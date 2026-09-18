use std::io;
use std::path::Path;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::options::mappings::{save_user_mappings, AliasMapping, UserMappings};
use crate::options::{merge_unknowns, OptionResolver, ResolverOptions};
use crate::pipeline::load_config;
use crate::registry::VendorRegistry;

pub fn run_map_tui(
    registry: &VendorRegistry,
    source_path: &Path,
    target_path: &Path,
    source_vendor: &str,
    target_vendor: &str,
    mapping_path: &Path,
) -> anyhow::Result<()> {
    let resolver = OptionResolver::load(Some(mapping_path), ResolverOptions::default())?;
    let source = load_config(registry, source_path, source_vendor, &resolver)?;
    let target = load_config(registry, target_path, target_vendor, &resolver)?;

    let unknowns = merge_unknowns([
        resolver.collect_unknowns(&source),
        resolver.collect_unknowns(&target),
    ]);

    if unknowns.is_empty() {
        println!("No unknown options found.");
        return Ok(());
    }

    let mut mappings = if mapping_path.exists() {
        crate::options::mappings::load_user_mappings(mapping_path)?
    } else {
        UserMappings::default()
    };

    let mut index = 0usize;
    let mut status = String::new();
    let mut canonical_input = String::new();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = (|| -> anyhow::Result<()> {
        loop {
            let current = &unknowns[index];
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(14),
                        Constraint::Min(4),
                        Constraint::Length(5),
                        Constraint::Length(3),
                    ])
                    .split(f.area());

                let info = format!(
                    "Unknown option {}/{}: {}\nUsages: {}\nExample: {:?}\nSites:\n{}",
                    index + 1,
                    unknowns.len(),
                    current.raw_name,
                    current.usage_count,
                    current.example_value,
                    current.sites.join("\n")
                );
                f.render_widget(
                    Paragraph::new(info)
                        .block(Block::default().title("Unknown Option").borders(Borders::ALL)),
                    chunks[0],
                );

                let items: Vec<ListItem> = mappings
                    .aliases
                    .iter()
                    .map(|a| {
                        ListItem::new(format!(
                            "{} -> {}:{}",
                            a.source_name, a.canonical.space, a.canonical.code
                        ))
                    })
                    .collect();
                f.render_widget(
                    List::new(items)
                        .block(Block::default().title("Current mappings").borders(Borders::ALL)),
                    chunks[1],
                );

                f.render_widget(
                    Paragraph::new(format!(
                        "Canonical code entry: {canonical_input}\n[i] ignore  [m] map to dhcp code  [n] next  [q] save & quit"
                    ))
                    .block(Block::default().title("Actions").borders(Borders::ALL)),
                    chunks[2],
                );

                f.render_widget(Paragraph::new(status.clone()), chunks[3]);
            })?;

            if event::poll(std::time::Duration::from_millis(200))? {
                match event::read()? {
                    Event::Resize(_, _) => {
                        // Redraw on next loop iteration with the new size.
                    }
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('n') => {
                            index = (index + 1) % unknowns.len();
                            canonical_input.clear();
                            status = "Next option".to_string();
                        }
                        KeyCode::Char('i') => {
                            let cur = &unknowns[index];
                            mappings.ignore.push(crate::options::OptionKeyRef {
                                space: cur.key.space.clone(),
                                code: cur.key.code,
                            });
                            status = format!("Ignored {}", cur.raw_name);
                            index = (index + 1) % unknowns.len();
                        }
                        KeyCode::Char('m') => {
                            if let Ok(code) = canonical_input.parse::<u16>() {
                                let cur = &unknowns[index];
                                mappings.aliases.push(AliasMapping {
                                    source_name: cur.raw_name.clone(),
                                    canonical: crate::options::OptionKeyRef {
                                        space: "dhcp".to_string(),
                                        code,
                                    },
                                    note: Some("mapped via TUI".to_string()),
                                });
                                status = format!("Mapped {} -> dhcp:{code}", cur.raw_name);
                                index = (index + 1) % unknowns.len();
                                canonical_input.clear();
                            } else {
                                status = "Enter numeric code before pressing m".to_string();
                            }
                        }
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            canonical_input.push(c);
                        }
                        KeyCode::Backspace => {
                            canonical_input.pop();
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    // Always restore the terminal, even if the loop failed.
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
    result?;

    save_user_mappings(mapping_path, &mappings).context("save mappings")?;
    println!("Mappings saved to {}", mapping_path.display());
    Ok(())
}
