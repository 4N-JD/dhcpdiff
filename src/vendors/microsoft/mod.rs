use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use regex::Regex;

use crate::formats::xml::parse_xml;
use crate::model::{
    BoundOption, Config, NormalizedValue, OptionKey, Pool, Reservation, SourceRef, Subnet,
};
use crate::registry::{
    DetectionScore, FormatFamily, Input, VendorDocument, VendorPlugin,
};

pub struct MicrosoftPlugin;

impl VendorPlugin for MicrosoftPlugin {
    fn id(&self) -> &'static str {
        "microsoft"
    }

    fn display_name(&self) -> &'static str {
        "Microsoft DHCP (Export-DhcpServer XML)"
    }

    fn format_family(&self) -> FormatFamily {
        FormatFamily::Xml
    }

    fn detect(&self, input: &Input) -> DetectionScore {
        let c = input.content.trim();
        if c.contains("<DHCPServer") || c.contains("DHCPv4Scope") || c.contains("DhcpScope") {
            DetectionScore::HIGH
        } else {
            DetectionScore::NONE
        }
    }

    fn parse(&self, input: &Input) -> anyhow::Result<VendorDocument> {
        let doc = parse_xml(&input.content)?;
        Ok(VendorDocument::Xml(doc))
    }

    fn normalize(&self, doc: VendorDocument, input: &Input) -> anyhow::Result<Config> {
        let VendorDocument::Xml(xml_doc) = doc else {
            anyhow::bail!("Microsoft plugin expects XML document");
        };
        normalize_microsoft(&xml_doc, &input.file_name())
    }
}

/// Minimal Microsoft plugin: parses Export-DhcpServer style XML into IR.
/// Full schema coverage is deferred; stub handles fixture scopes.
fn normalize_microsoft(
    doc: &crate::formats::xml::XmlDocument,
    file: &str,
) -> anyhow::Result<Config> {
    let content = &doc.content;
    if !content.contains("Scope") && !content.contains("scope") {
        anyhow::bail!(
            "Microsoft plugin: no DHCP scopes found in XML (full parser not yet implemented for this export)"
        );
    }

    let mut config = Config::default();
    parse_microsoft_scopes(content, file, &mut config)?;
    parse_microsoft_reservations(content, file, &mut config)?;
    parse_microsoft_options(content, &mut config);
    Ok(config)
}

fn parse_microsoft_scopes(content: &str, file: &str, config: &mut Config) -> anyhow::Result<()> {
    let scope_re = Regex::new(
        r#"<ScopeId>([^<]+)</ScopeId>\s*<SubnetMask>([^<]+)</SubnetMask>"#,
    )?;

    for cap in scope_re.captures_iter(content) {
        let network: Ipv4Addr = cap[1].parse()?;
        let netmask: Ipv4Addr = cap[2].parse()?;
        let network = Ipv4Net::with_netmask(network, netmask)?;

        let mut subnet = Subnet {
            network,
            shared_network: None,
            options: BTreeMap::new(),
            pools: Vec::new(),
            reservations: Vec::new(),
            filters: Vec::new(),
            extensions: BTreeMap::new(),
            source: Some(SourceRef::new("microsoft", file, 1)),
        };

        if let (Some(start), Some(end)) = (parse_xml_value(content, "StartRange"), parse_xml_value(content, "EndRange")) {
            if let (Ok(start), Ok(end)) = (start.parse::<Ipv4Addr>(), end.parse::<Ipv4Addr>()) {
                subnet.pools.push(Pool {
                    start,
                    end,
                    options: BTreeMap::new(),
                    extensions: BTreeMap::new(),
                    source: Some(SourceRef::new("microsoft", file, 1)),
                });
            }
        }

        config.subnets.push(subnet);
    }
    Ok(())
}

fn parse_xml_value(content: &str, tag: &str) -> Option<String> {
    let re = Regex::new(&format!(r"<{tag}>([^<]+)</{tag}>")).ok()?;
    re.captures(content).map(|c| c[1].to_string())
}

fn parse_microsoft_reservations(content: &str, file: &str, config: &mut Config) -> anyhow::Result<()> {
    let ip_re = Regex::new(r#"(?s)<Reservation[^>]*>\s*<IPAddress>([^<]+)</IPAddress>"#)?;
    let id_re = Regex::new(r#"(?s)<Reservation[^>]*>.*?<ClientId>([^<]+)</ClientId>"#)?;

    let ips: Vec<Ipv4Addr> = ip_re
        .captures_iter(content)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let macs: Vec<String> = id_re
        .captures_iter(content)
        .map(|c| crate::model::normalize_mac(&c[1]))
        .collect();

    for (ip, mac) in ips.into_iter().zip(macs) {
        let subnet_idx = config
            .subnets
            .iter()
            .position(|s| s.network.contains(&ip))
            .or(if config.subnets.is_empty() { None } else { Some(0) });
        if let Some(idx) = subnet_idx {
            config.subnets[idx].reservations.push(Reservation {
                mac,
                ip,
                options: BTreeMap::new(),
                extensions: BTreeMap::new(),
                source: Some(SourceRef::new("microsoft", file, 1)),
            });
        }
    }
    Ok(())
}

fn parse_microsoft_options(content: &str, config: &mut Config) {
    let opt_re = Regex::new(r#"<OptionId>(\d+)</OptionId>\s*<Value>([^<]+)</Value>"#).ok();
    if let Some(re) = opt_re {
        for cap in re.captures_iter(content) {
            let code: u16 = cap[1].parse().unwrap_or(0);
            let value = BoundOption::from(NormalizedValue::from_raw_text(&cap[2]));
            if let Some(subnet) = config.subnets.first_mut() {
                subnet.options.insert(OptionKey::dhcp(code), value);
            } else {
                config.global_options.insert(OptionKey::dhcp(code), value);
            }
        }
    }
}

