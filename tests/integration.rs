use std::path::PathBuf;

use dhcpdiff::diff::DiffEntry;
use dhcpdiff::model::{ClientScenario, FilterMatch, OptionKey, RuleScope};
use dhcpdiff::options::inherit::{
    effective_client_options, effective_pool_options, effective_reservation_options,
};
use dhcpdiff::options::scenario::discover_scenarios;
use dhcpdiff::options::{OptionResolver, ResolverOptions};
use dhcpdiff::registry::{Input, VendorRegistry};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn load(vendor: &str, path: PathBuf) -> dhcpdiff::model::Config {
    let registry = VendorRegistry::new();
    let resolver = OptionResolver::load(None, ResolverOptions::default()).unwrap();
    let input = Input::from_path(path).unwrap();
    let mut config = registry.parse_and_normalize(vendor, &input).unwrap();
    resolver.resolve_config(&mut config);
    config
}

fn pool_option_diffs(report: &dhcpdiff::diff::DiffReport) -> Vec<&DiffEntry> {
    report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
            | DiffEntry::Changed { entity, .. } if entity.kind == "option" && entity.key.starts_with("pool:")))
        .collect()
}

fn reservation_diffs(report: &dhcpdiff::diff::DiffReport) -> Vec<&DiffEntry> {
    report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
            | DiffEntry::Changed { entity, .. } if entity.kind == "reservation"))
        .collect()
}

fn reservation_option_diffs(report: &dhcpdiff::diff::DiffReport) -> Vec<&DiffEntry> {
    report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
            | DiffEntry::Changed { entity, .. }
            if entity.kind == "option" && entity.key.starts_with("reservation:")))
        .collect()
}

#[test]
fn parse_qip_fixture() {
    let registry = VendorRegistry::new();
    let path = fixture("paired_qip.conf");
    let input = Input::from_path(path).unwrap();
    let config = registry.parse_and_normalize("qip", &input).unwrap();

    assert!(config.subnet_count() >= 1);
    assert!(config.pool_count() >= 1);
}

#[test]
fn parse_infoblox_fixture() {
    let registry = VendorRegistry::new();
    let path = fixture("option_space_ibx.conf");
    let input = Input::from_path(path).unwrap();
    let config = registry.parse_and_normalize("infoblox", &input).unwrap();

    assert_eq!(config.subnet_count(), 1);
    assert_eq!(config.pool_count(), 1);
    assert_eq!(config.option_definitions.len(), 3);
}

#[test]
fn parse_bluecat_fixture() {
    let registry = VendorRegistry::new();
    let path = fixture("detect_bluecat.conf");
    let input = Input::from_path(path).unwrap();
    let config = registry.parse_and_normalize("bluecat", &input).unwrap();

    assert_eq!(config.subnet_count(), 1);
    assert_eq!(config.pool_count(), 1);
    assert_eq!(config.global_filters.len(), 1);
}

#[test]
fn auto_detect_bluecat_fixture() {
    let registry = VendorRegistry::new();
    let bluecat = Input::from_path(fixture("detect_bluecat.conf")).unwrap();
    assert_eq!(registry.detect(&bluecat).unwrap().id(), "bluecat");
}

#[test]
fn bluecat_option_space_not_treated_as_unknown() {
    let registry = VendorRegistry::new();
    let input = Input::from_path(fixture("detect_bluecat.conf")).unwrap();
    let config = registry.parse_and_normalize("bluecat", &input).unwrap();
    let resolver = dhcpdiff::options::OptionResolver::load(None, ResolverOptions::default()).unwrap();

    assert!(
        !config.global_options.keys().any(|k| k.is_unresolved()),
        "option space declarations must not become unknown global options"
    );
    assert!(
        resolver.collect_unknowns(&config).is_empty(),
        "bluecat fixture should have no unknown options after space fix"
    );
}

#[test]
fn auto_detect_qip_and_infoblox() {
    let registry = VendorRegistry::new();
    let qip = Input::from_path(fixture("paired_qip.conf")).unwrap();
    let infoblox = Input::from_path(fixture("option_space_ibx.conf")).unwrap();

    assert_eq!(registry.detect(&qip).unwrap().id(), "qip");
    assert_eq!(registry.detect(&infoblox).unwrap().id(), "infoblox");

    let bluecat = Input::from_path(fixture("detect_bluecat.conf")).unwrap();
    assert_eq!(registry.detect(&bluecat).unwrap().id(), "bluecat");
}

#[test]
fn parse_microsoft_minimal_fixture() {
    let registry = VendorRegistry::new();
    let path = fixture("microsoft/minimal.xml");
    let input = Input::from_path(path).unwrap();
    let config = registry.parse_and_normalize("microsoft", &input).unwrap();

    assert_eq!(config.subnet_count(), 1);
    assert_eq!(config.pool_count(), 1);
    assert_eq!(config.reservation_count(), 1);
}

#[test]
fn paired_configs_diff_minimal() {
    let registry = VendorRegistry::new();
    let qip = Input::from_path(fixture("paired_qip.conf")).unwrap();
    let infoblox = Input::from_path(fixture("paired_infoblox.conf")).unwrap();

    let source = registry.parse_and_normalize("qip", &qip).unwrap();
    let target = registry.parse_and_normalize("infoblox", &infoblox).unwrap();

    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let missing_subnets: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, dhcpdiff::diff::DiffEntry::MissingInTarget { entity, .. } if entity.kind == "subnet"))
        .collect();
    assert!(missing_subnets.is_empty(), "expected no missing subnets: {:?}", report.entries);
}

#[test]
fn infoblox_option_space_not_treated_as_unknown() {
    let registry = VendorRegistry::new();
    let input = Input::from_path(fixture("option_space_ibx.conf")).unwrap();
    let config = registry.parse_and_normalize("infoblox", &input).unwrap();
    let resolver = dhcpdiff::options::OptionResolver::load(None, ResolverOptions::default()).unwrap();

    assert!(
        !config.global_options.keys().any(|k| k.is_unresolved()),
        "option space declarations must not become unknown global options"
    );
    assert_eq!(
        config.option_definitions.len(),
        3,
        "vendor option definitions should still be collected"
    );
    assert!(
        resolver.collect_unknowns(&config).is_empty(),
        "infoblox fixture should have no unknown options after space fix"
    );
}

#[test]
fn unresolved_option_names_are_preserved() {
    let resolver = dhcpdiff::options::OptionResolver::load(None, ResolverOptions::default()).unwrap();
    let mut config = dhcpdiff::model::Config::default();
    config.global_options.insert(
        OptionKey::unresolved("log-servers"),
        dhcpdiff::model::NormalizedValue::Ip("10.1.10.34".parse().unwrap()).into(),
    );
    config.global_options.insert(
        OptionKey::unresolved("cookie-servers"),
        dhcpdiff::model::NormalizedValue::Ip("10.1.10.35".parse().unwrap()).into(),
    );

    let unknowns = resolver.collect_unknowns(&config);
    assert_eq!(unknowns.len(), 2);
    let names: Vec<_> = unknowns.iter().map(|u| u.raw_name.as_str()).collect();
    assert!(names.contains(&"log-servers"));
    assert!(names.contains(&"cookie-servers"));
    assert_eq!(
        unknowns
            .iter()
            .find(|u| u.raw_name == "log-servers")
            .unwrap()
            .key,
        OptionKey::unresolved("log-servers")
    );
}

#[test]
fn user_alias_resolves_unresolved_option_name() {
    let mut resolver =
        dhcpdiff::options::OptionResolver::load(None, ResolverOptions::default()).unwrap();
    resolver.apply_user_mappings(dhcpdiff::options::mappings::UserMappings {
        aliases: vec![dhcpdiff::options::mappings::AliasMapping {
            source_name: "log-servers".to_string(),
            canonical: dhcpdiff::options::OptionKeyRef {
                space: "dhcp".to_string(),
                code: 7,
            },
            note: None,
        }],
        ..Default::default()
    });

    let mut config = dhcpdiff::model::Config::default();
    config.global_options.insert(
        OptionKey::unresolved("log-servers"),
        dhcpdiff::model::NormalizedValue::Ip("10.1.10.34".parse().unwrap()).into(),
    );
    resolver.resolve_config(&mut config);

    assert!(config.global_options.contains_key(&OptionKey::dhcp(7)));
    assert!(!config
        .global_options
        .contains_key(&OptionKey::unresolved("log-servers")));
    assert!(resolver.collect_unknowns(&config).is_empty());
}

#[test]
fn legacy_bare_unknown_still_collected() {
    let resolver = dhcpdiff::options::OptionResolver::load(None, ResolverOptions::default()).unwrap();
    let mut config = dhcpdiff::model::Config::default();
    config.global_options.insert(
        dhcpdiff::model::OptionKey {
            space: "unknown".to_string(),
            code: 0,
        },
        dhcpdiff::model::NormalizedValue::Int(1).into(),
    );
    let unknowns = resolver.collect_unknowns(&config);
    assert_eq!(unknowns.len(), 1);
    assert_eq!(unknowns[0].raw_name, "unknown:0");
}

#[test]
fn effective_pool_options_inherit_global() {
    let config = load("infoblox", fixture("inherit_ibx.conf"));
    let subnet = &config.subnets[0];
    let pool = &subnet.pools[0];
    let effective = effective_pool_options(&config, subnet, pool);

    assert!(effective.contains_key(&OptionKey::dhcp(15)));
    match &effective.get(&OptionKey::dhcp(15)).unwrap().value {
        dhcpdiff::model::NormalizedValue::String(s) => assert_eq!(s, "lab.4n.de"),
        other => panic!("expected string domain-name, got {other:?}"),
    }
    assert!(effective.contains_key(&OptionKey::dhcp(3)));
}

#[test]
fn diff_pool_inherited_vs_explicit_matches() {
    let source = load("infoblox", fixture("inherit_ibx.conf"));
    let target = load("qip", fixture("inherit_qip.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let pool_option_diffs = pool_option_diffs(&report);
    assert!(
        pool_option_diffs.is_empty(),
        "expected no pool option diffs with inheritance: {:?}",
        report.entries
    );
}

#[test]
fn diff_pool_inherited_value_change_detected() {
    let source = load("infoblox", fixture("inherit_ibx.conf"));
    let target = load("infoblox", fixture("inherit_ibx_changed.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let changed: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, .. }
            if entity.kind == "option" && entity.key.contains("dhcp:15")))
        .collect();
    assert!(!changed.is_empty(), "global domain-name change should surface at pool scope");
}

#[test]
fn shared_network_options_inherited() {
    let config = load("infoblox", fixture("inherit_shared_network.conf"));
    assert_eq!(config.shared_networks.len(), 1);
    let subnet = &config.subnets[0];
    assert_eq!(subnet.shared_network.as_deref(), Some("corp"));
    let effective = effective_pool_options(&config, subnet, &subnet.pools[0]);
    match &effective.get(&OptionKey::dhcp(15)).unwrap().value {
        dhcpdiff::model::NormalizedValue::String(s) => assert_eq!(s, "shared.example.com"),
        other => panic!("expected string domain-name, got {other:?}"),
    }
}

#[test]
fn reservation_inherits_subnet_options() {
    let config = load("infoblox", fixture("inherit_host.conf"));
    let subnet = &config.subnets[0];
    let res = &subnet.reservations[0];
    let effective = effective_reservation_options(&config, subnet, res);
    assert!(effective.contains_key(&OptionKey::dhcp(3)));
}

#[test]
fn infoblox_global_host_assigned_to_subnet() {
    let config = load("infoblox", fixture("reservation_global.conf"));
    assert_eq!(config.reservation_count(), 1);
    let res = &config.subnets[0].reservations[0];
    assert_eq!(res.ip.to_string(), "10.97.128.15");
    assert_eq!(res.mac, "aa:bb:cc:dd:ee:01");
}

#[test]
fn diff_global_vs_subnet_host_matches_by_ip() {
    let source = load("infoblox", fixture("reservation_global.conf"));
    let target = load("infoblox", fixture("reservation_subnet.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    assert!(
        reservation_diffs(&report).is_empty(),
        "global and subnet-nested hosts with same IP should match: {:?}",
        report.entries
    );
}

#[test]
fn diff_reservation_inherited_options_match() {
    let source = load("infoblox", fixture("reservation_global_options.conf"));
    let target = load("infoblox", fixture("reservation_subnet_options.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    assert!(
        reservation_option_diffs(&report).is_empty(),
        "inherited vs explicit reservation options should match: {:?}",
        report.entries
    );
}

#[test]
fn diff_reservation_mac_change_detected() {
    let source = load("infoblox", fixture("reservation_mac_a.conf"));
    let target = load("infoblox", fixture("reservation_mac_b.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    let mac_changes: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, field, .. }
            if entity.kind == "reservation" && field == "mac"))
        .collect();
    assert!(
        !mac_changes.is_empty(),
        "same IP with different MAC should be reported: {:?}",
        report.entries
    );
}

#[test]
fn bootp_params_inherit_in_bootp_space() {
    let config = load("infoblox", fixture("inherit_bootp_global.conf"));
    let subnet = &config.subnets[0];
    let effective = effective_pool_options(&config, subnet, &subnet.pools[0]);
    assert!(effective.contains_key(&OptionKey::bootp_filename()));
    assert!(effective.contains_key(&OptionKey::bootp_next_server()));
    assert!(effective.contains_key(&OptionKey::bootp_server_name()));
    assert!(
        !effective.contains_key(&OptionKey::dhcp(67)),
        "filename must not become dhcp:67: {:?}",
        effective.keys().collect::<Vec<_>>()
    );
    assert!(
        !effective.contains_key(&OptionKey::dhcp(66)),
        "server-name must not become dhcp:66: {:?}",
        effective.keys().collect::<Vec<_>>()
    );
    assert!(
        !effective.contains_key(&OptionKey::dhcp(150)),
        "next-server must not become dhcp:150: {:?}",
        effective.keys().collect::<Vec<_>>()
    );
}

fn load_with_mappings(
    vendor: &str,
    path: PathBuf,
    mappings: dhcpdiff::options::mappings::UserMappings,
) -> dhcpdiff::model::Config {
    let registry = VendorRegistry::new();
    let mut resolver = OptionResolver::load(None, ResolverOptions::default()).unwrap();
    resolver.apply_user_mappings(mappings);
    let input = Input::from_path(path).unwrap();
    let mut config = registry.parse_and_normalize(vendor, &input).unwrap();
    resolver.resolve_config(&mut config);
    config
}

#[test]
fn ignore_subnet_mask_via_mapping() {
    let mappings = dhcpdiff::options::mappings::UserMappings {
        ignore: vec![dhcpdiff::options::OptionKeyRef {
            space: "dhcp".to_string(),
            code: 1,
        }],
        ..Default::default()
    };
    let source = load_with_mappings(
        "infoblox",
        fixture("ignore_subnet_mask_a.conf"),
        mappings.clone(),
    );
    let target = load_with_mappings(
        "infoblox",
        fixture("ignore_subnet_mask_b.conf"),
        mappings,
    );
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let subnet_mask_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
            if entity.kind == "option" && entity.key.contains("dhcp:1")))
        .collect();
    assert!(
        subnet_mask_diffs.is_empty(),
        "subnet-mask should be ignored when dhcp:1 is in ignore: {:?}",
        subnet_mask_diffs
    );
}

#[test]
fn ignore_subnet_mask_absent_when_not_in_mapping() {
    let source = load("infoblox", fixture("ignore_subnet_mask_a.conf"));
    let target = load("infoblox", fixture("ignore_subnet_mask_b.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    let subnet_mask_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, .. }
            if entity.kind == "option" && entity.key.contains("dhcp:1")))
        .collect();
    assert!(
        !subnet_mask_diffs.is_empty(),
        "subnet-mask diff should appear when dhcp:1 is not ignored"
    );
}

#[test]
fn legacy_ignore_subnet_mask_flag_migrates_to_ignore() {
    let mut resolver = OptionResolver::load(None, ResolverOptions::default()).unwrap();
    resolver.apply_user_mappings(dhcpdiff::options::mappings::UserMappings {
        ignore_subnet_mask: Some(true),
        ..Default::default()
    });
    let registry = VendorRegistry::new();
    let mut source = registry
        .parse_and_normalize(
            "infoblox",
            &Input::from_path(fixture("ignore_subnet_mask_a.conf")).unwrap(),
        )
        .unwrap();
    let mut target = registry
        .parse_and_normalize(
            "infoblox",
            &Input::from_path(fixture("ignore_subnet_mask_b.conf")).unwrap(),
        )
        .unwrap();
    resolver.resolve_config(&mut source);
    resolver.resolve_config(&mut target);
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    let subnet_mask_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
            if entity.kind == "option" && entity.key.contains("dhcp:1")))
        .collect();
    assert!(
        subnet_mask_diffs.is_empty(),
        "legacy ignore_subnet_mask: true should ignore dhcp:1: {:?}",
        subnet_mask_diffs
    );
}

#[test]
fn diff_output_includes_option_names() {
    let source = load("infoblox", fixture("inherit_ibx.conf"));
    let target = load("infoblox", fixture("inherit_ibx_changed.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let named: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { field, .. } if field.contains("domain-name")))
        .collect();
    assert!(
        !named.is_empty(),
        "expected option name in diff output: {:?}",
        report.entries
    );
}

#[test]
fn bootp_params_not_equivalent_to_dhcp_options() {
    // Global BOOTP statements must not match pool-level DHCP options 66/67.
    let source = load("infoblox", fixture("inherit_bootp_global.conf"));
    let target = load("infoblox", fixture("inherit_bootp_explicit.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    let pool_opts = pool_option_diffs(&report);
    assert!(
        pool_opts.iter().any(|e| matches!(
            e,
            DiffEntry::ExtraInTarget { entity, .. } if entity.key.contains("dhcp:66")
                || entity.key.contains("dhcp:67")
        )),
        "DHCP options on target should remain distinct from BOOTP source: {:?}",
        report.entries
    );
    assert!(
        pool_opts.iter().any(|e| matches!(
            e,
            DiffEntry::MissingInTarget { entity, .. } if entity.key.contains("bootp:")
        )),
        "BOOTP fields present only on source should be missing in target: {:?}",
        report.entries
    );
}

#[test]
fn bootp_params_match_bootp_to_bootp() {
    let source = load("infoblox", fixture("inherit_bootp_global.conf"));
    let target = load("infoblox", fixture("inherit_bootp_matching.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);
    assert!(
        pool_option_diffs(&report).is_empty(),
        "identical BOOTP fields should match across scopes: {:?}",
        report.entries
    );
}

#[test]
fn parse_global_if_blocks_into_conditional_rules() {
    let config = load("infoblox", fixture("scenario/aastra_global_if_ibx.conf"));
    assert!(
        !config.conditional_rules.is_empty(),
        "expected global if blocks to become conditional rules"
    );
    assert!(matches!(
        config.conditional_rules[0].match_expr,
        FilterMatch::VendorClassExact { ref value } if value == "AastraIPPhone"
    ));
}

#[test]
fn discover_scenarios_finds_vci_from_if_and_class() {
    let config = load("infoblox", fixture("scenario/class_vs_if_ibx.conf"));
    let scenarios = discover_scenarios(&config);
    assert!(scenarios.iter().any(|s| s.vendor_class.is_none()));
    assert!(scenarios
        .iter()
        .any(|s| s.vendor_class.as_deref() == Some("AastraIPPhone")));
}

#[test]
fn filter_match_vendor_class_exact() {
    let m = FilterMatch::VendorClassExact {
        value: "AastraIPPhone".to_string(),
    };
    assert!(m.matches(&ClientScenario::with_vendor_class("AastraIPPhone")));
    assert!(!m.matches(&ClientScenario::baseline()));
    assert!(!m.matches(&ClientScenario::with_vendor_class("Other")));
}

#[test]
fn vendor_options_gated_without_matching_vci() {
    let config = load("bluecat", fixture("scenario/aastra_vci_gated.conf"));
    let subnet = &config.subnets[0];
    let res = &subnet.reservations[0];

    let baseline = effective_client_options(
        &config,
        subnet,
        None,
        Some(res),
        &ClientScenario::baseline(),
    );
    assert!(
        !baseline.keys().any(|k| k.space == "AastraIPPhone"),
        "baseline must not include gated vendor options: {:?}",
        baseline.keys().collect::<Vec<_>>()
    );

    let with_vci = effective_client_options(
        &config,
        subnet,
        None,
        Some(res),
        &ClientScenario::with_vendor_class("AastraIPPhone"),
    );
    assert!(
        with_vci.keys().any(|k| k.space == "AastraIPPhone"),
        "VCI scenario should include AastraIPPhone options: {:?}",
        with_vci.keys().collect::<Vec<_>>()
    );
}

#[test]
fn filter_match_vendor_class_prefix_with_spaces() {
    use dhcpdiff::model::FilterMatch;
    let m = FilterMatch::VendorClassPrefix {
        value: "PXEClient".to_string(),
        offset: 0,
        length: 9,
    };
    assert!(m.matches(&ClientScenario::with_vendor_class("PXEClient")));
    assert!(m.matches(&ClientScenario::with_vendor_class("PXEClient:Arch:00000")));
    assert!(!m.matches(&ClientScenario::baseline()));
}

#[test]
fn pxeclient_substring_global_if_applies_dhcp_options() {
    let config = load("bluecat", fixture("scenario/pxeclient_substring_bcn.conf"));
    let global_rule = config
        .conditional_rules
        .iter()
        .find(|r| matches!(r.scope, RuleScope::Global))
        .expect("global PXEClient rule");
    assert!(
        matches!(
            global_rule.match_expr,
            FilterMatch::VendorClassPrefix { .. }
        ),
        "BlueCat spaced substring if should parse as VendorClassPrefix: {:?}",
        global_rule.match_expr
    );

    let subnet = &config.subnets[0];
    let res = &subnet.reservations[0];
    let with_pxe = effective_client_options(
        &config,
        subnet,
        None,
        Some(res),
        &ClientScenario::with_vendor_class("PXEClient"),
    );
    assert!(
        with_pxe.contains_key(&OptionKey::dhcp(54)),
        "PXEClient scenario should include dhcp-server-identifier (54): {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
    assert!(
        with_pxe.contains_key(&OptionKey::bootp_filename()),
        "PXEClient scenario should include filename as bootp: {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
    assert!(
        with_pxe.contains_key(&OptionKey::bootp_next_server()),
        "PXEClient scenario should include next-server as bootp: {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
    assert!(
        !with_pxe.contains_key(&OptionKey::dhcp(67)),
        "filename BOOTP field must not become dhcp:67: {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
    assert!(
        !with_pxe.contains_key(&OptionKey::dhcp(150)),
        "next-server BOOTP field must not become dhcp:150: {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        with_pxe.get(&OptionKey::isc_default_lease_time()).map(|b| &b.value),
        Some(&dhcpdiff::model::NormalizedValue::Int(30)),
        "PXEClient scenario should apply default-lease-time 30 as isc:0: {:?}",
        with_pxe.get(&OptionKey::isc_default_lease_time())
    );
    assert!(
        !with_pxe.contains_key(&OptionKey::dhcp(51)),
        "default-lease-time must not become dhcp:51: {:?}",
        with_pxe.keys().collect::<Vec<_>>()
    );
}

#[test]
fn pxeclient_substring_bcn_matches_ibx_if() {
    let source = load("bluecat", fixture("scenario/pxeclient_substring_bcn.conf"));
    let target = load("infoblox", fixture("scenario/pxeclient_substring_ibx.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let pxe_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option" && entity.key.contains("vci=PXEClient") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        pxe_diffs.is_empty(),
        "BlueCat substring if should match Infoblox exact if for PXEClient: {:?}",
        pxe_diffs
    );
}

#[test]
fn scenario_diff_only_reports_scenario_delta_options() {
    let source = load("bluecat", fixture("scenario/opti_lease_bcn.conf"));
    let target = load("infoblox", fixture("scenario/opti_lease_ibx.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let baseline_lease: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
                if entity.kind == "option"
                    && entity.key.starts_with("reservation:10.0.0.15:")
                    && !entity.key.contains(":vci=")
                    && entity.key.contains("isc:0") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert_eq!(
        baseline_lease.len(),
        1,
        "baseline should report global lease-time mismatch: {:?}",
        report.entries
    );

    let vci_lease: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option"
                    && entity.key.contains("vci=OptiIpPhone")
                    && entity.key.contains("isc:0") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        vci_lease.is_empty(),
        "VCI scenario should not re-report unchanged lease time: {:?}",
        vci_lease
    );

    let vci_vendor: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
                if entity.kind == "option" && entity.key.contains("vci=OptiIpPhone:OptiIpPhone:") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        !vci_vendor.is_empty(),
        "VCI scenario should still report vendor-option deltas: {:?}",
        report.entries
    );
}

#[test]
fn isc_lease_time_statements_are_distinct_keys() {
    let config = load("infoblox", fixture("isc_lease_times.conf"));
    assert_eq!(
        config
            .global_options
            .get(&OptionKey::isc_default_lease_time())
            .map(|b| &b.value),
        Some(&dhcpdiff::model::NormalizedValue::Int(3600))
    );
    assert_eq!(
        config
            .global_options
            .get(&OptionKey::isc_min_lease_time())
            .map(|b| &b.value),
        Some(&dhcpdiff::model::NormalizedValue::Int(600))
    );
    assert_eq!(
        config
            .global_options
            .get(&OptionKey::isc_max_lease_time())
            .map(|b| &b.value),
        Some(&dhcpdiff::model::NormalizedValue::Int(7200))
    );
    assert!(
        !config.global_options.contains_key(&OptionKey::dhcp(51)),
        "lease-time statements must not collapse into dhcp:51: {:?}",
        config.global_options.keys().collect::<Vec<_>>()
    );
}

#[test]
fn scenario_ibx_if_matches_bcn_if_for_vci() {
    let source = load("infoblox", fixture("scenario/aastra_global_if_ibx.conf"));
    let target = load("bluecat", fixture("scenario/aastra_global_if_bcn.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let vci_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option" && entity.key.contains("vci=AastraIPPhone") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        vci_diffs.is_empty(),
        "Infoblox if + BlueCat if/global vendor option should match for VCI: {:?}",
        vci_diffs
    );
}

#[test]
fn scenario_class_vs_if_equivalent() {
    let source = load("infoblox", fixture("scenario/class_vs_if_ibx.conf"));
    let target = load("bluecat", fixture("scenario/class_vs_if_bcn.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let vci_option_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option" && entity.key.contains("vci=AastraIPPhone") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        vci_option_diffs.is_empty(),
        "class options should match if-block options for same VCI: {:?}",
        vci_option_diffs
    );
}

#[test]
fn equivalence_rewrites_vendor_option_space_for_scenario_gating() {
    // BlueCat MSFT50 vs Infoblox Microsoft-Windows-Options: equivalences must
    // remap both option keys and vendor-option-space so VCI gating still allows
    // the remapped options on the source side.
    let registry = VendorRegistry::new();
    let mut resolver = OptionResolver::load(None, ResolverOptions::default()).unwrap();
    resolver.apply_user_mappings(dhcpdiff::options::mappings::UserMappings {
        equivalences: vec![dhcpdiff::options::mappings::EquivalenceMapping {
            source: dhcpdiff::options::OptionKeyRef {
                space: "MSFT50".into(),
                code: 1,
            },
            target: dhcpdiff::options::OptionKeyRef {
                space: "Microsoft-Windows-Options".into(),
                code: 1,
            },
            confirmed: true,
        }],
        ..Default::default()
    });

    let mut source = registry
        .parse_and_normalize(
            "bluecat",
            &Input::from_path(fixture("scenario/msft_space_bcn.conf")).unwrap(),
        )
        .unwrap();
    let mut target = registry
        .parse_and_normalize(
            "infoblox",
            &Input::from_path(fixture("scenario/msft_space_ibx.conf")).unwrap(),
        )
        .unwrap();
    resolver.resolve_config(&mut source);
    resolver.resolve_config(&mut target);

    assert_eq!(
        source.conditional_rules[0].vendor_option_space.as_deref(),
        Some("Microsoft-Windows-Options"),
        "vendor-option-space should follow space equivalences"
    );

    let report = dhcpdiff::diff::diff_configs(&source, &target);
    let msft_option_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option"
                    && (entity.key.contains("MSFT50")
                        || entity.key.contains("Microsoft-Windows-Options")
                        || entity.key.contains("vci=MSFT 5.0")) =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        msft_option_diffs.is_empty(),
        "MSFT50↔Microsoft-Windows-Options should match under VCI after equivalence: {:?}",
        msft_option_diffs
    );
}

#[test]
fn scenario_pool_vci_option_change_detected() {
    let source = load("infoblox", fixture("scenario/pool_vci_a.conf"));
    let target = load("infoblox", fixture("scenario/pool_vci_b.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let changed: Vec<_> = report
        .entries
        .iter()
        .filter(|e| matches!(e, DiffEntry::Changed { entity, .. }
            if entity.kind == "option" && entity.key.contains("vci=AastraIPPhone")))
        .collect();
    assert!(
        !changed.is_empty(),
        "pool VCI scenario should detect cfg-server-name change: {:?}",
        report.entries
    );
}

#[test]
fn scenario_detects_real_aastra_mismatch() {
    let source = load("infoblox", fixture("scenario/aastra_mismatch_ibx.conf"));
    let target = load("bluecat", fixture("scenario/aastra_mismatch_bcn.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let vci_diffs: Vec<_> = report
        .entries
        .iter()
        .filter(|e| match e {
            DiffEntry::Changed { entity, .. }
            | DiffEntry::MissingInTarget { entity, .. }
            | DiffEntry::ExtraInTarget { entity, .. }
                if entity.kind == "option"
                    && entity.key.contains("192.168.167.219")
                    && entity.key.contains("vci=AastraIPPhone") =>
            {
                true
            }
            _ => false,
        })
        .collect();
    assert!(
        !vci_diffs.is_empty(),
        "expected Aastra cfg-server-name mismatch for reservation under VCI: {:?}",
        report.entries
    );
}

#[test]
fn json_report_is_machine_ready_for_layout_a() {
    let source = load("infoblox", fixture("inherit_ibx.conf"));
    let target = load("infoblox", fixture("inherit_ibx_changed.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target).with_file_meta(
        dhcpdiff::diff::FileMeta {
            path: "inherit_ibx.conf".into(),
            vendor: "infoblox".into(),
        },
        dhcpdiff::diff::FileMeta {
            path: "inherit_ibx_changed.conf".into(),
            vendor: "infoblox".into(),
        },
    );

    assert_eq!(report.version, 1);
    assert!(report.counts.total > 0);
    assert_eq!(
        report.counts.total,
        report.counts.missing
            + report.counts.extra
            + report.counts.changed
            + report.counts.unmapped
    );
    assert_eq!(report.source.as_ref().unwrap().vendor, "infoblox");

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["version"], 1);
    assert!(json["counts"]["total"].as_u64().unwrap() > 0);

    let changed = report
        .entries
        .iter()
        .find(|e| {
            matches!(
                e,
                DiffEntry::Changed { entity, .. }
                    if entity.kind == "option" && entity.key.contains("dhcp:15")
            )
        })
        .expect("expected domain-name change");

    match changed {
        DiffEntry::Changed {
            entity,
            detail,
            values,
            locations,
            ..
        } => {
            assert!(!detail.is_empty());
            assert!(matches!(
                values.source,
                dhcpdiff::model::NormalizedValue::String(_)
            ));
            assert!(matches!(
                values.target,
                dhcpdiff::model::NormalizedValue::String(_)
            ));
            let src_loc = locations.source.as_ref().expect("source location on pool option");
            assert!(src_loc.affected.line > 0);
            assert!(src_loc.affected.end_line.is_some());
            assert!(src_loc.affected.end_line.unwrap() >= src_loc.affected.line);
            let tgt_loc = locations.target.as_ref().expect("target location on pool option");
            assert!(tgt_loc.affected.end_line.is_some());
            // Inherited domain-name is declared globally, not on the pool block.
            let src_decl = src_loc.declaration.as_ref().expect("source declaration");
            assert_eq!(src_decl.line, 1);
            assert_eq!(
                entity.display.as_ref().and_then(|d| d.declared_in.as_deref()),
                Some("Global")
            );
        }
        _ => unreachable!(),
    }

    let entry_json = serde_json::to_value(changed).unwrap();
    assert_eq!(entry_json["category"], "changed");
    assert_eq!(entry_json["values"]["source"]["type"], "String");
}

#[test]
fn normalized_entities_carry_end_line_spans() {
    let config = load("infoblox", fixture("inherit_ibx.conf"));
    let subnet = &config.subnets[0];
    let src = subnet.source.as_ref().expect("subnet source");
    assert_eq!(src.line, 3);
    assert_eq!(src.end_line, Some(9));

    let pool = &subnet.pools[0];
    let pool_src = pool.source.as_ref().expect("pool source");
    assert_eq!(pool_src.line, 5);
    assert_eq!(pool_src.end_line, Some(8));
}

#[test]
fn option_diff_carries_declaration_vs_affected_for_subnet_if() {
    let source = load("bluecat", fixture("scenario/provenance_subnet_if_bcn.conf"));
    let target = load("infoblox", fixture("scenario/provenance_global_if_ibx.conf"));
    let report = dhcpdiff::diff::diff_configs(&source, &target);

    let changed = report
        .entries
        .iter()
        .find(|e| {
            matches!(
                e,
                DiffEntry::Changed { entity, .. }
                    if entity.kind == "option"
                        && entity.key.contains("vci=PXEClient")
                        && entity.key.contains("next-server")
            )
        })
        .expect("expected PXE next-server pool VCI diff");

    match changed {
        DiffEntry::Changed {
            entity,
            locations,
            ..
        } => {
            let src = locations.source.as_ref().expect("source side");
            let tgt = locations.target.as_ref().expect("target side");

            // Affected = pool blocks
            assert_eq!(src.affected.line, 5);
            assert_eq!(src.affected.end_line, Some(7));
            assert_eq!(tgt.affected.line, 6);
            assert_eq!(tgt.affected.end_line, Some(8));

            // Declaration = next-server statements inside if rules
            let src_decl = src.declaration.as_ref().expect("source declaration");
            let tgt_decl = tgt.declaration.as_ref().expect("target declaration");
            assert_eq!(src_decl.line, 3, "BlueCat next-server inside subnet if");
            assert_eq!(tgt_decl.line, 2, "Infoblox next-server inside global if");

            assert_eq!(
                entity.display.as_ref().and_then(|d| d.declared_in.as_deref()),
                Some("Subnet if PXEClient")
            );
            let json = serde_json::to_value(locations).unwrap();
            assert!(json["source"]["affected"]["line"].is_number());
            assert!(json["source"]["declaration"]["line"].is_number());
        }
        _ => unreachable!(),
    }
}

#[test]
fn inherited_global_option_declaration_on_global_statement() {
    let config = load("infoblox", fixture("inherit_ibx.conf"));
    let subnet = &config.subnets[0];
    let pool = &subnet.pools[0];
    let effective = effective_pool_options(&config, subnet, pool);
    let bound = effective.get(&OptionKey::dhcp(15)).expect("domain-name");
    assert_eq!(bound.source.as_ref().map(|s| s.line), Some(1));
    assert_eq!(bound.declared_in.as_deref(), Some("Global"));
    assert_ne!(
        bound.source.as_ref().map(|s| s.line),
        pool.source.as_ref().map(|s| s.line),
        "declaration must not be the pool block"
    );
}
