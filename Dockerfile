FROM rust:1.88.0-slim-bookworm

WORKDIR /workspace

RUN rustup component add clippy rustfmt

RUN useradd --create-home --uid 10001 platform

COPY --chown=platform:platform . .

USER platform

CMD ["bash", "scripts/quality.sh"]
