FROM rust:1.87-alpine3.21 AS base 

ARG CARGO_FEATURES="blog"

FROM base AS install
WORKDIR /temp/dev
COPY Cargo.toml Cargo.lock .
RUN mkdir src && touch src/lib.rs
RUN mkdir .cargo && cargo vendor --locked >> .cargo/config.toml

FROM base AS builder
WORKDIR /usr/src/app
COPY --from=install /temp/dev/vendor /temp/dev/.cargo .
COPY . .
RUN cargo build --release --no-default-features --features $CARGO_FEATURES

FROM scratch AS runner
USER runner
EXPOSE 80/tcp 22/tcp
COPY --from=builder /usr/src/app/target/release/ssh-portfolio /usr/local/bin/ssh-portfolio

CMD ["/usr/local/bin/ssh-portfolio"]
