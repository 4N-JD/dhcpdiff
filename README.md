# dhcpdiff

Multi-vendor DHCP configuration diff tool for migration validation. Parses vendor-specific DHCP configs into a unified intermediate representation (IR), resolves options by DHCP code, and reports client-facing differences.

## Supported vendors

| Vendor | Format | Plugin ID |
|--------|--------|-----------|
| QIP | ISC-derived `dhcpd.conf` | `qip` |
| Infoblox | ISC-derived `dhcpd.conf` | `infoblox` |
| BlueCat | ISC-derived `dhcpd.conf` | `bluecat` |
| Microsoft DHCP | `Export-DhcpServer` XML (stub) | `microsoft` |

Use `--vendor auto` to detect the format automatically.

## Build

```bash
cargo build --release
```

## Usage

List plugins:

```bash
dhcpdiff vendors
```

Normalize a config to unified JSON:

```bash
dhcpdiff normalize --input tests/fixtures/paired_qip.conf --vendor qip -o qip.json
```

Compare source and target configs:

```bash
dhcpdiff diff \
  --source tests/fixtures/paired_qip.conf --source-vendor qip \
  --target tests/fixtures/paired_infoblox.conf --target-vendor infoblox \
  --ignore-unmapped
```

Interactively map unknown options (writes `mappings/user.yaml`):

```bash
dhcpdiff map --source source.conf --target target.conf \
  --source-vendor qip --target-vendor infoblox
```

## Web UI

A separate FastAPI app under [`web/`](web/) provides Layout A (master–detail) diffs, config uploads, CLI option toggles, and a YAML mapping editor (replacement for the interactive `map` TUI).

Large configs are handled via a **job store**: uploads are kept on disk with a line-offset index, the diff report is stored server-side, and the browser loads **paginated entries** plus **line windows** for the dual file panes (virtualized scrolling). Responses no longer embed full file bodies.

### Local run

```bash
cargo build --release
export DHCPDIFF_BIN="$PWD/target/release/dhcpdiff"
python3 -m venv web/.venv
web/.venv/bin/pip install -r web/requirements.txt
cd web && PYTHONPATH=. .venv/bin/uvicorn app.main:app --host 127.0.0.1 --port 8080
```

Open http://127.0.0.1:8080

Optional environment variables:

| Variable | Default | Purpose |
|----------|---------|---------|
| `DHCPDIFF_BIN` | `dhcpdiff` | Path to the CLI binary |
| `DHCPDIFF_DEFAULT_MAPPING` | (repo/`/app` mappings) | Default `user.yaml` contents for the editor |
| `DHCPDIFF_JOB_DIR` | `/tmp/dhcpdiff-jobs` | Where uploaded configs and reports are stored |
| `DHCPDIFF_JOB_TTL_HOURS` | `24` | Expired jobs are removed on the next create |

### Docker

```bash
docker compose up --build
```

Then open http://127.0.0.1:8080. Compose builds the image from the Dockerfile, publishes port 8080, and keeps job files in a named volume.

Manual build/run is still available:

```bash
docker build -t dhcpdiff-web .
docker run --rm -p 8080:8080 dhcpdiff-web
```

The image includes the `dhcpdiff` CLI and the web UI. Set `DHCPDIFF_BIN` only if you override the binary path. Job files default to `/tmp/dhcpdiff-jobs` inside the container.

## Option mapping

Built-in RFC 2132 option names live in [`mappings/builtin.yaml`](mappings/builtin.yaml). User overrides and cross-vendor aliases go in `mappings/user.yaml`:

```yaml
aliases:
  - source_name: dns-servers
    canonical: { space: dhcp, code: 6 }
equivalences:
  - source: { space: BLABLA, code: 1 }
    target: { space: dhcp, code: 220 }
    confirmed: true
ignore:
  - { space: dhcp, code: 999 }
  - { space: isc, code: 1 }   # ignore min-lease-time only
  - { space: isc, code: 2 }   # ignore max-lease-time only
# Ignore dhcp option 1 (subnet-mask); redundant with subnet netmask declaration.
ignore_subnet_mask: true
```

`ignore` entries are `{ space, code }` pairs matching the IR keys shown in diffs (for example `isc:0 (default-lease-time)` or `dhcp:6 (domain-name-servers)`). Add any option you want excluded from normalize/diff to that list.

Confirmed equivalences also rewrite `vendor-option-space` when every mapped code for a source space shares the same target space (for example `MSFT50` → `Microsoft-Windows-Options`), so VCI scenario gating stays aligned with remapped option keys.

## Option inheritance

ISC DHCP (Infoblox, BlueCat, and generic `dhcpd.conf`) inherits options and BOOTP packet fields down the scope hierarchy unless overridden:

**Global → Shared Network → Subnet → Pool → Host**

BOOTP statements (`filename`, `next-server`, `server-name`) are stored in a separate `bootp` space and compared **only against other BOOTP fields**. They are never folded into DHCP options 66, 67, or 150. Use `option tftp-server-name` / `option bootfile-name` / vendor option 150 when you mean those DHCP options.

ISC server lease statements (`default-lease-time`, `min-lease-time`, `max-lease-time`) are stored in a separate `isc` space (`isc:0`, `isc:1`, `isc:2`) and compared only against each other. They are **not** folded into DHCP option 51. Wire option 51 remains `option dhcp-lease-time`. Ignore individual lease statements via the `ignore` list (see above).

Pool and reservation diffs compare **effective** (fully inherited) client-facing values, so a global `option domain-name` on Infoblox matches an explicit pool-level `option domain-name` on QIP when the values are the same.

QIP configs typically set all options explicitly at pool or reservation level; inheritance mainly benefits cross-vendor migration diffs against Infoblox/BlueCat.

**Reservations** (`host` / `fixed-address`) are matched globally by **fixed IP**, regardless of whether the host block is declared at file top level (BlueCat style) or inside a `subnet` block (Infoblox style). Global hosts are normalized into the containing subnet by IP during parse. Option diffs compare effective inherited values using each side's subnet and pool context.

### Client scenarios (vendor class)

Diff also evaluates **client scenarios** so conditional rules (`class`, `if` / `elsif`, QIP `vendor-class`) are applied the way a DHCP client would see them.

- Scenarios are **auto-discovered** from vendor-class identifiers found in filters and `if` blocks (plus a baseline with no VCI).
- Effective options for each scenario: static inheritance, then matching filters/conditionals (last wins), then **vendor-space gating** — non-`dhcp` options (from global, subnet, pool, or host) are included only when a matching rule set `vendor-option-space` to that space (or declared options inside a matching filter/conditional).
- Diff scopes look like `reservation:10.0.0.15:vci=AastraIPPhone` and `pool:10.0.0.0/24:10.0.0.10-10.0.0.20:vci=AastraIPPhone` (baseline omits the `:vci=` suffix).

Limitations: opaque/`and`/`or` match expressions are only partially evaluated; CLI `--vendor-class` overrides are not implemented yet; `allow members of` class membership is not modeled.

For large configs, build with `--release` (`cargo run --release -- diff ...`). Diff prints progress to stderr while parsing and comparing.

**Subnet mask (dhcp option 1)** is ignored by default during diff and normalize, since it is usually implied by the subnet declaration. Disable with `--no-ignore-subnet-mask` on the CLI or `ignore_subnet_mask: false` in `mappings/user.yaml`.

## Adding a new vendor

1. Implement `VendorPlugin` in `src/vendors/<name>/`
2. Reuse a format family (`formats/isc/` or `formats/xml/`) where possible
3. Map constructs to the unified IR in `src/model/`
4. Register the plugin in `VendorRegistry`
5. Add fixtures under `tests/fixtures/<name>/`

The diff engine, option resolver, and reporters are vendor-agnostic and should not need changes.

## Architecture

```
Vendor file → VendorPlugin (parse + normalize) → Config IR → OptionResolver → Diff → Report
```

## Tests

```bash
cargo test
```
