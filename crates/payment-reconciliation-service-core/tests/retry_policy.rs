use payment_reconciliation_service_core::RetryPolicy;

#[test]
fn exponential_delay_is_bounded_and_alert_threshold_is_explicit() {
    let policy = RetryPolicy::new(100, 20, 100, 3).unwrap();
    assert_eq!(policy.pending_delay_ms(), 100);
    assert_eq!(policy.error_delay_ms(1), 20);
    assert_eq!(policy.error_delay_ms(2), 40);
    assert_eq!(policy.error_delay_ms(3), 80);
    assert_eq!(policy.error_delay_ms(4), 100);
    assert_eq!(policy.error_delay_ms(u32::MAX), 100);
    assert!(!policy.should_alert(2));
    assert!(policy.should_alert(3));
}

#[test]
fn zero_or_inverted_retry_configuration_is_rejected() {
    assert!(RetryPolicy::new(0, 1, 1, 1).is_none());
    assert!(RetryPolicy::new(1, 0, 1, 1).is_none());
    assert!(RetryPolicy::new(1, 2, 1, 1).is_none());
    assert!(RetryPolicy::new(1, 1, 1, 0).is_none());
}
