use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{AccountId, NetworkId};
use payment_idempotency_core::TenantId;
use payment_ingress_core::{
    ApiPrincipalId, BucketPolicy, PaymentRateLimitConfig, RateLimitDecision, RateLimitError,
    RateLimitScope, TrackingCapacities,
};
use payment_rate_limit_lmdb::{
    LmdbPaymentRateLimiter, LmdbRateLimitError, LmdbRateLimitOptions, MINIMUM_MAP_SIZE,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const CHILD_PATH: &str = "PAYMENT_RATE_LIMIT_CHILD_PATH";
const CHILD_MODE: &str = "PAYMENT_RATE_LIMIT_CHILD_MODE";
const CHILD_OUTPUT: &str = "PAYMENT_RATE_LIMIT_CHILD_OUTPUT";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(label: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-rate-limit-{label}-{}-{sequence}",
        std::process::id()
    ))
}

fn policy(capacity: u32) -> BucketPolicy {
    BucketPolicy::new(capacity, 100).unwrap()
}

fn config(
    principal: u32,
    tenant: u32,
    account: u32,
    tracking: TrackingCapacities,
) -> PaymentRateLimitConfig {
    let longest_recovery = u64::from(principal.max(tenant).max(account)) * 100;
    PaymentRateLimitConfig::new(
        policy(principal),
        policy(tenant),
        policy(account),
        tracking,
        longest_recovery,
    )
    .unwrap()
}

fn shared_config() -> PaymentRateLimitConfig {
    config(
        1,
        1,
        1,
        TrackingCapacities {
            principals: 8,
            tenants: 8,
            accounts: 8,
        },
    )
}

fn open(path: &Path, config: PaymentRateLimitConfig) -> LmdbPaymentRateLimiter {
    LmdbPaymentRateLimiter::open(path, NETWORK, config, LmdbRateLimitOptions::default()).unwrap()
}

fn principal(marker: u8) -> ApiPrincipalId {
    ApiPrincipalId::new([marker; 32])
}

fn child_command(path: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--exact")
        .arg("child_process_observes_shared_principal_bucket")
        .arg("--nocapture")
        .env(CHILD_PATH, path)
        .env(CHILD_MODE, mode);
    command
}

fn run_child(path: &Path, mode: &str) {
    let output = child_command(path, mode).output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn separate_processes_share_one_atomic_principal_bucket() {
    let path = directory("process");
    let limiter = open(&path, shared_config());
    assert_eq!(
        limiter.admit_principal(principal(7), 0),
        Ok(RateLimitDecision::Admitted)
    );

    run_child(&path, "limited");
    run_child(&path, "admitted");
    assert_eq!(
        limiter.admit_principal(principal(7), 100),
        Ok(RateLimitDecision::Limited {
            scope: RateLimitScope::ApiPrincipal,
            retry_at_ms: 200,
        })
    );

    drop(limiter);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn child_process_observes_shared_principal_bucket() {
    let Ok(path) = std::env::var(CHILD_PATH) else {
        return;
    };
    let mode = std::env::var(CHILD_MODE).unwrap();
    let limiter = open(Path::new(&path), shared_config());
    if mode == "race" {
        let result = limiter.admit_principal(principal(10), 0).unwrap();
        let marker = match result {
            RateLimitDecision::Admitted => b"admitted".as_slice(),
            RateLimitDecision::Limited { .. } => b"limited".as_slice(),
        };
        fs::write(std::env::var(CHILD_OUTPUT).unwrap(), marker).unwrap();
        return;
    }
    if mode == "tenant-limited" {
        assert_eq!(
            limiter.admit_new_intent(TenantId::new([1; 32]), AccountId::new([4; 32]), 0),
            Ok(RateLimitDecision::Limited {
                scope: RateLimitScope::Tenant,
                retry_at_ms: 100,
            })
        );
        return;
    }
    let (now_ms, expected) = match mode.as_str() {
        "limited" => (
            0,
            RateLimitDecision::Limited {
                scope: RateLimitScope::ApiPrincipal,
                retry_at_ms: 100,
            },
        ),
        "admitted" => (100, RateLimitDecision::Admitted),
        _ => panic!("unexpected child mode"),
    };
    assert_eq!(limiter.admit_principal(principal(7), now_ms), Ok(expected));
}

#[test]
fn concurrent_processes_cannot_both_spend_the_last_token() {
    let path = directory("race");
    let output_directory = directory("race-results");
    fs::create_dir_all(&output_directory).unwrap();
    let first_output = output_directory.join("first");
    let second_output = output_directory.join("second");
    let mut first = child_command(&path, "race")
        .env(CHILD_OUTPUT, &first_output)
        .spawn()
        .unwrap();
    let mut second = child_command(&path, "race")
        .env(CHILD_OUTPUT, &second_output)
        .spawn()
        .unwrap();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());

    let mut outcomes = [
        fs::read_to_string(first_output).unwrap(),
        fs::read_to_string(second_output).unwrap(),
    ];
    outcomes.sort();
    assert_eq!(outcomes, ["admitted", "limited"]);
    let limiter = open(&path, shared_config());
    assert_eq!(
        limiter.tracked_identities(RateLimitScope::ApiPrincipal),
        Ok(1)
    );

    drop(limiter);
    fs::remove_dir_all(path).unwrap();
    fs::remove_dir_all(output_directory).unwrap();
}

#[test]
fn tenant_and_account_charge_is_atomic_across_processes() {
    let path = directory("atomic");
    let first = open(&path, shared_config());
    let tenant_one = TenantId::new([1; 32]);
    let tenant_two = TenantId::new([2; 32]);
    let account_one = AccountId::new([3; 32]);
    let account_two = AccountId::new([4; 32]);

    assert_eq!(
        first.admit_new_intent(tenant_one, account_one, 0),
        Ok(RateLimitDecision::Admitted)
    );
    run_child(&path, "tenant-limited");
    assert_eq!(first.tracked_identities(RateLimitScope::Account), Ok(1));
    assert_eq!(
        first.admit_new_intent(tenant_two, account_two, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(first.tracked_identities(RateLimitScope::Tenant), Ok(2));
    assert_eq!(first.tracked_identities(RateLimitScope::Account), Ok(2));

    drop(first);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn restart_preserves_limits_policy_binding_and_logical_clock_floor() {
    let path = directory("restart");
    let policy = shared_config();
    let limiter = open(&path, policy);
    assert_eq!(
        limiter.admit_principal(principal(8), 100),
        Ok(RateLimitDecision::Admitted)
    );
    drop(limiter);

    let reopened = open(&path, policy);
    assert_eq!(
        reopened.admit_principal(principal(8), 100),
        Ok(RateLimitDecision::Limited {
            scope: RateLimitScope::ApiPrincipal,
            retry_at_ms: 200,
        })
    );
    assert_eq!(
        reopened.admit_principal(principal(8), 99),
        Ok(RateLimitDecision::Limited {
            scope: RateLimitScope::ApiPrincipal,
            retry_at_ms: 200,
        })
    );
    drop(reopened);
    assert!(matches!(
        LmdbPaymentRateLimiter::open(
            &path,
            NetworkId::new([2; 32]),
            policy,
            LmdbRateLimitOptions::default(),
        ),
        Err(LmdbRateLimitError::WrongNetwork)
    ));
    let changed = config(
        2,
        1,
        1,
        TrackingCapacities {
            principals: 8,
            tenants: 8,
            accounts: 8,
        },
    );
    assert!(matches!(
        LmdbPaymentRateLimiter::open(&path, NETWORK, changed, LmdbRateLimitOptions::default(),),
        Err(LmdbRateLimitError::ConfigurationMismatch)
    ));

    fs::remove_dir_all(path).unwrap();
}

#[test]
fn interleaved_request_phases_use_the_durable_logical_time_floor() {
    let path = directory("interleaved-time");
    let limiter = open(&path, shared_config());
    assert_eq!(
        limiter.admit_principal(principal(1), 100),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_principal(principal(2), 101),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_new_intent(TenantId::new([1; 32]), AccountId::new([1; 32]), 100),
        Ok(RateLimitDecision::Admitted)
    );

    drop(limiter);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn bounded_tracking_reuses_only_a_fully_recovered_expired_slot() {
    let path = directory("expiry");
    let policy = config(
        2,
        1,
        1,
        TrackingCapacities {
            principals: 1,
            tenants: 1,
            accounts: 1,
        },
    );
    let limiter = open(&path, policy);
    assert_eq!(
        limiter.admit_principal(principal(1), 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_principal(principal(2), 199),
        Err(LmdbRateLimitError::RateLimit(
            RateLimitError::TrackingCapacityReached(RateLimitScope::ApiPrincipal)
        ))
    );
    assert_eq!(
        limiter.admit_principal(principal(2), 200),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.tracked_identities(RateLimitScope::ApiPrincipal),
        Ok(1)
    );

    drop(limiter);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn unsafe_small_map_and_time_overflow_fail_without_bucket_mutation() {
    let path = directory("bounds");
    assert!(matches!(
        LmdbPaymentRateLimiter::open(
            &path,
            NETWORK,
            shared_config(),
            LmdbRateLimitOptions {
                map_size: MINIMUM_MAP_SIZE - 1,
                ..LmdbRateLimitOptions::default()
            },
        ),
        Err(LmdbRateLimitError::MapSizeTooSmall)
    ));

    let limiter = open(&path, shared_config());
    assert_eq!(
        limiter.admit_principal(principal(1), u64::MAX),
        Err(LmdbRateLimitError::RateLimit(RateLimitError::TimeOverflow))
    );
    assert_eq!(
        limiter.tracked_identities(RateLimitScope::ApiPrincipal),
        Ok(0)
    );

    drop(limiter);
    fs::remove_dir_all(path).unwrap();
}
