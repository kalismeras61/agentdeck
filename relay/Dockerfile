# AgentDeck Relay: docker build -t agentdeck-relay relay/ && docker run -p 8787:8787 -v relay-data:/data agentdeck-relay
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:stable-slim
RUN useradd --system --home /data relay && mkdir -p /data && chown relay /data
COPY --from=build /src/target/release/agentdeck-relay /usr/local/bin/agentdeck-relay
USER relay
ENV AGENTDECK_RELAY_DB=/data/agentdeck-relay.db
EXPOSE 8787
VOLUME /data
ENTRYPOINT ["agentdeck-relay"]
CMD ["serve", "--addr", "0.0.0.0:8787"]
