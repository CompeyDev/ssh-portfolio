# --- Web build layer ---
FROM oven/bun:1.2-alpine AS www
WORKDIR /usr/src/www
COPY www/package.json www/bun.lock .
RUN bun install
COPY www .
RUN bun run build

# --- Rust build layer ---
FROM rust:1.87-alpine3.21 AS builder

# Install prerequisites
RUN apk add --no-cache \
  build-base \
  git \
  pkgconfig \
  openssl-dev \
  openssl-libs-static \
  zlib-dev \
  zlib-static \
  shadow \
  libcap-utils
RUN cargo install patch-crate --locked

ARG CARGO_FEATURES="blog"

# Cache dependency artifacts to prevent recompilation on future builds
WORKDIR /usr/src/app
COPY rust-toolchain Cargo.toml Cargo.lock .
COPY patches patches
RUN mkdir src \
  && touch src/lib.rs \
  && echo "fn main() {}" > build.rs \
  && cargo patch-crate \
  && cargo build --locked --release --no-default-features ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} 

# Compile the real source code
COPY . .
COPY --from=www /usr/src/www/build www/build
RUN touch build.rs \
  && SKIP_PATCH_CRATE=1 cargo build --locked --release --no-default-features ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} \
  && strip ./target/release/ssh-portfolio \
  && cp ./target/release/ssh-portfolio /usr/local/bin/ # must be moved to a secure path to preserve caps

# Create a user without root permissions & set binary capabilities
RUN adduser -u 1000 --disabled-password runner
RUN setcap CAP_NET_BIND_SERVICE=+eip /usr/local/bin/ssh-portfolio

# --- Runner layer ---
FROM scratch AS runner

# De-escalate priveleges to non-root user
COPY --from=builder /home/runner /home/runner
COPY --from=builder /etc/passwd /etc/passwd
USER 1000

# Copy compiled binary over
COPY --from=builder /usr/local/bin/ssh-portfolio /usr/local/bin/ssh-portfolio

# Start server
EXPOSE 80/tcp 22/tcp
CMD ["/usr/local/bin/ssh-portfolio", "--host", "0.0.0.0"]
