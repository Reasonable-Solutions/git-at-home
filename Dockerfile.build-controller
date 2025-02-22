FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release --package build-controller --workspace

FROM debian:latest
WORKDIR /app
COPY --from=builder /app/target/release/build-controller .

CMD ["./build-controller"]
