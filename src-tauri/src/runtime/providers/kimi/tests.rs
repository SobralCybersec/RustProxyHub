use super::model_scenario;

#[test]
fn k2_models_map_to_k2d5_scenario() {
    assert_eq!(model_scenario("kimi-k2.6").scenario, "SCENARIO_K2D5");
    assert_eq!(
        model_scenario("kimi-k2.6-thinking").scenario,
        "SCENARIO_K2D5"
    );
    assert_eq!(model_scenario("k2d6").scenario, "SCENARIO_K2D5");
    assert_eq!(model_scenario("k2d6-thinking").scenario, "SCENARIO_K2D5");
}

#[test]
fn k2_thinking_variants_set_thinking_flag() {
    assert!(!model_scenario("kimi-k2.6").thinking);
    assert!(model_scenario("kimi-k2.6-thinking").thinking);
    assert!(!model_scenario("k2d6").thinking);
    assert!(model_scenario("k2d6-thinking").thinking);
}

#[test]
fn k3_models_map_to_k3_scenario() {
    assert_eq!(model_scenario("kimi-k3").scenario, "SCENARIO_K3");
    assert_eq!(model_scenario("kimi-k3-thinking").scenario, "SCENARIO_K3");
    assert_eq!(model_scenario("kimi-latest").scenario, "SCENARIO_K3");
}

#[test]
fn k3_thinking_variant_sets_thinking_flag() {
    assert!(!model_scenario("kimi-k3").thinking);
    assert!(model_scenario("kimi-k3-thinking").thinking);
    assert!(!model_scenario("kimi-latest").thinking);
}

#[test]
fn agent_models_map_to_ok_computer_scenario() {
    let agent = model_scenario("k2d6-agent");
    assert_eq!(agent.scenario, "SCENARIO_OK_COMPUTER");
    assert_eq!(agent.kimi_plus_id.as_deref(), Some("ok-computer"));
    assert_eq!(agent.agent_mode.as_deref(), Some("TYPE_NORMAL"));

    let ultra = model_scenario("k2d6-agent-ultra");
    assert_eq!(ultra.scenario, "SCENARIO_OK_COMPUTER");
    assert_eq!(ultra.agent_mode.as_deref(), Some("TYPE_ULTRA"));
}

#[test]
fn no_thinking_suffix_stripped_before_matching() {
    // "kimi-k2.6-no-thinking" should clean to "kimi-k2.6" and match
    let cfg = model_scenario("kimi-k2.6-no-thinking");
    assert_eq!(cfg.scenario, "SCENARIO_K2D5");
    assert!(!cfg.thinking);
}

#[test]
fn unknown_model_falls_through_to_k2d5_default() {
    let cfg = model_scenario("future-unknown-model");
    assert_eq!(cfg.scenario, "SCENARIO_K2D5");
    assert!(!cfg.thinking);
}

#[test]
fn unknown_thinking_model_falls_through_with_thinking_true() {
    let cfg = model_scenario("future-unknown-model-thinking");
    assert!(cfg.thinking);
}
