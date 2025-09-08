# --- Web build layer ---
FROM oven/bun:1.2-alpine AS www
WORKDIR /usr/src/www
COPY www/package.json www/bun.lock .
RUN bun install --locked
COPY www .
RUN bun run build

# --- Rust build layer ---
FROM rust:1.87-alpine3.21 AS builder

# Install prerequisites
RUN apk add --no-cache \
  build-base \
  git \
  pkgconfig \
  zlib-dev \
  zlib-static \
  shadow \
  libcap-utils \
  bash

ARG CARGO_FEATURES="blog"

# Cache dependency artifacts to prevent recompilation on future builds
WORKDIR /usr/src/app
COPY .cargo/ ./.cargo/
COPY patches/ ./patches/
COPY rust-toolchain Cargo.toml Cargo.lock .
RUN mkdir src \
  && touch src/lib.rs \
  && echo "fn main() {}" > build.rs \
  && cargo build --locked --release --no-default-features ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} 

# Compile the real source code
COPY . .
RUN mkdir www/build
COPY --from=www /usr/src/www/build www/build
RUN touch build.rs \
  && cargo build --locked --release --no-default-features ${CARGO_FEATURES:+--features "$CARGO_FEATURES"} \
  && strip ./target/release/ssh-portfolio \
  && cp ./target/release/ssh-portfolio /usr/local/bin/ # must be moved to a secure path to preserve caps

# Create a user without root permissions & set binary capabilities
RUN adduser -u 1000 --disabled-password runner
RUN setcap CAP_NET_BIND_SERVICE=+eip /usr/local/bin/ssh-portfolio

# --- Runner layer ---
FROM scratch AS runner

# De-escalate priveleges to non-root user
COPY --from=builder --chown=1000 /home/runner /home/runner
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /usr/local/bin/ssh-portfolio /usr/local/bin/ssh-portfolio
USER 1000 

# Start server
EXPOSE 80/tcp 22/tcp
CMD ["--host", "0.0.0.0"]
ENTRYPOINT ["/usr/local/bin/ssh-portfolio"]
