# syntax=docker/dockerfile:1
FROM rust:1.80-slim AS build
ARG PROFILE=release
WORKDIR /src
COPY . .
RUN cargo build --profile ${PROFILE} \
    && strip target/release/app

FROM debian:bookworm-slim
ENV RUST_LOG=info PORT=8080
COPY --from=build /src/target/release/app /usr/local/bin/app
EXPOSE 8080
HEALTHCHECK CMD curl -f http://localhost:8080/health || exit 1
ENTRYPOINT ["/usr/local/bin/app"]
CMD ["--port", "8080"]
