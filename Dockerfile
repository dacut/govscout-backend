FROM rust:1-bookworm as builder
RUN mkdir /build
COPY ["Cargo.toml", "Cargo.lock", "/build/"]
COPY ["src", "/build/src/"]
WORKDIR /build
RUN cargo build --release
RUN ln /build/target/release/govscout-backend /bootstrap

FROM public.ecr.aws/lambda/provided:al2023
COPY --from=builder /bootstrap /var/task/
