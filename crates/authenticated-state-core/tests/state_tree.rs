use std::fmt::Write;

use authenticated_state_core::{
    AuthenticatedStateError, AuthenticatedStateRoot, AuthenticatedStateTree, MAX_STATE_KEY_BYTES,
    MAX_STATE_VALUE_BYTES, StateEntry, StateMutation, StateNamespace,
};

const ACCOUNTS: StateNamespace = StateNamespace::new(1);
const ASSETS: StateNamespace = StateNamespace::new(2);

fn entry(namespace: StateNamespace, key: &[u8], value: &[u8]) -> StateEntry {
    StateEntry::new(namespace, key.to_vec(), value.to_vec())
}

fn root_hex(root: AuthenticatedStateRoot) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in root.as_bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[test]
fn canonical_root_is_order_independent_namespace_bound_and_stable() {
    let entries = vec![
        entry(ACCOUNTS, b"alice", b"100"),
        entry(ACCOUNTS, b"bob", b"200"),
        entry(ASSETS, b"alice", b"active"),
    ];
    let (_, first) = AuthenticatedStateTree::create(entries.clone()).unwrap();
    let (_, reversed) =
        AuthenticatedStateTree::create(entries.into_iter().rev().collect()).unwrap();
    let (_, other_namespace) = AuthenticatedStateTree::create(vec![
        entry(ACCOUNTS, b"alice", b"100"),
        entry(ACCOUNTS, b"bob", b"200"),
        entry(ACCOUNTS, b"alice-status", b"active"),
    ])
    .unwrap();

    assert_eq!(first, reversed);
    assert_ne!(first, other_namespace);
    assert_eq!(
        root_hex(first),
        "829bbcec40bdd4545722c0e90a1ccdb6d2f9728642d43a5053e4a52e16b82f85"
    );
}

#[test]
fn incremental_set_and_delete_match_a_fresh_rebuild() {
    let (mut tree, initial_root) = AuthenticatedStateTree::create(vec![
        entry(ACCOUNTS, b"alice", b"100"),
        entry(ACCOUNTS, b"bob", b"200"),
    ])
    .unwrap();
    let updated_root = tree
        .apply(
            1,
            vec![
                StateMutation::set(ACCOUNTS, b"alice".to_vec(), b"125".to_vec()),
                StateMutation::delete(ACCOUNTS, b"bob".to_vec()),
                StateMutation::set(ACCOUNTS, b"carol".to_vec(), b"75".to_vec()),
            ],
        )
        .unwrap();
    let (_, rebuilt_root) = AuthenticatedStateTree::create(vec![
        entry(ACCOUNTS, b"alice", b"125"),
        entry(ACCOUNTS, b"carol", b"75"),
    ])
    .unwrap();

    assert_ne!(updated_root, initial_root);
    assert_eq!(updated_root, rebuilt_root);
    assert_eq!(tree.root(0).unwrap(), initial_root);
    assert_eq!(tree.root(1).unwrap(), updated_root);
}

#[test]
fn retained_versions_prove_membership_and_nonmembership() {
    let (mut tree, old_root) =
        AuthenticatedStateTree::create(vec![entry(ACCOUNTS, b"alice", b"100")]).unwrap();
    let new_root = tree
        .apply(
            1,
            vec![StateMutation::set(
                ACCOUNTS,
                b"alice".to_vec(),
                b"125".to_vec(),
            )],
        )
        .unwrap();

    let (old_value, old_proof) = tree.get_with_proof(0, ACCOUNTS, b"alice").unwrap();
    assert_eq!(old_value.as_deref(), Some(b"100".as_slice()));
    assert!(old_proof.verifies(old_root, ACCOUNTS, b"alice", Some(b"100")));
    assert!(!old_proof.verifies(old_root, ACCOUNTS, b"alice", Some(b"101")));
    assert!(!old_proof.verifies(new_root, ACCOUNTS, b"alice", Some(b"100")));

    let (missing, missing_proof) = tree.get_with_proof(1, ACCOUNTS, b"bob").unwrap();
    assert!(missing.is_none());
    assert!(missing_proof.verifies(new_root, ACCOUNTS, b"bob", None));
    assert!(!missing_proof.verifies(new_root, ACCOUNTS, b"bob", Some(b"0")));
}

#[test]
fn prepared_updates_are_side_effect_free_and_only_one_competitor_can_commit() {
    let (mut tree, initial_root) =
        AuthenticatedStateTree::create(vec![entry(ACCOUNTS, b"alice", b"100")]).unwrap();
    let winner = tree
        .prepare(
            1,
            vec![StateMutation::set(
                ACCOUNTS,
                b"alice".to_vec(),
                b"125".to_vec(),
            )],
        )
        .unwrap();
    let loser = tree
        .prepare(
            1,
            vec![StateMutation::set(
                ACCOUNTS,
                b"alice".to_vec(),
                b"130".to_vec(),
            )],
        )
        .unwrap();

    assert_eq!(tree.latest_version(), Some(0));
    assert_eq!(tree.root(0).unwrap(), initial_root);
    let winner_root = winner.root();
    assert_eq!(tree.commit(winner).unwrap(), winner_root);
    assert_eq!(
        tree.commit(loser),
        Err(AuthenticatedStateError::InvalidVersion)
    );
    let (value, proof) = tree.get_with_proof(1, ACCOUNTS, b"alice").unwrap();
    assert_eq!(value.as_deref(), Some(b"125".as_slice()));
    assert!(proof.verifies(winner_root, ACCOUNTS, b"alice", Some(b"125")));
}

#[test]
fn rejected_version_duplicate_and_bounds_leave_tree_unchanged() {
    let (mut tree, root) =
        AuthenticatedStateTree::create(vec![entry(ACCOUNTS, b"alice", b"100")]).unwrap();
    let nodes = tree.retained_node_count();
    assert_eq!(
        tree.apply(2, vec![StateMutation::delete(ACCOUNTS, b"alice".to_vec())]),
        Err(AuthenticatedStateError::InvalidVersion)
    );
    assert_eq!(
        tree.apply(
            1,
            vec![
                StateMutation::delete(ACCOUNTS, b"alice".to_vec()),
                StateMutation::set(ACCOUNTS, b"alice".to_vec(), b"101".to_vec()),
            ]
        ),
        Err(AuthenticatedStateError::DuplicateKey)
    );
    assert_eq!(
        tree.apply(1, vec![StateMutation::delete(ACCOUNTS, Vec::new())]),
        Err(AuthenticatedStateError::EmptyKey)
    );
    assert_eq!(
        tree.apply(
            1,
            vec![StateMutation::delete(
                ACCOUNTS,
                vec![0; MAX_STATE_KEY_BYTES + 1],
            )]
        ),
        Err(AuthenticatedStateError::KeyTooLarge)
    );
    assert_eq!(
        tree.apply(
            1,
            vec![StateMutation::set(
                ACCOUNTS,
                b"alice".to_vec(),
                vec![0; MAX_STATE_VALUE_BYTES + 1],
            )]
        ),
        Err(AuthenticatedStateError::ValueTooLarge)
    );
    assert_eq!(tree.latest_version(), Some(0));
    assert_eq!(tree.root(0).unwrap(), root);
    assert_eq!(tree.retained_node_count(), nodes);
}
