# syntax=docker/dockerfile:1

FROM rust:1-slim-trixie AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY mappings ./mappings
RUN cargo build --release

FROM python:3.14-slim-trixie
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-build /src/target/release/dhcpdiff /usr/local/bin/dhcpdiff
COPY mappings /app/mappings
COPY web/requirements.txt /app/web/requirements.txt
RUN pip install --no-cache-dir -r /app/web/requirements.txt
COPY web /app/web

ENV DHCPDIFF_BIN=/usr/local/bin/dhcpdiff
ENV DHCPDIFF_DEFAULT_MAPPING=/app/mappings/user.yaml
ENV PYTHONPATH=/app/web

EXPOSE 8080
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8080"]
