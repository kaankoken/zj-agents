#[test]
fn elapsed_ms_boundaries() {
    assert_eq!(zj_agents_engine::elapsed_ms(1.0), 1_000);
    assert_eq!(zj_agents_engine::elapsed_ms(0.125), 125);
    assert_eq!(zj_agents_engine::elapsed_ms(-1.0), 0);
    assert_eq!(zj_agents_engine::elapsed_ms(f64::NAN), 0);
    assert_eq!(
        zj_agents_engine::elapsed_ms((u64::MAX as f64) / 500.0),
        u64::MAX
    );
}
