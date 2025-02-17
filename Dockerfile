##############################
## Build Rust backend
##############################
FROM rust:latest AS rust-builder

RUN update-ca-certificates

RUN apt-get update && apt-get install -y libclang-dev

WORKDIR /ebills

# start - build dependency cache
RUN mkdir ./src
RUN echo 'fn main() { panic!("Dummy Image Called!")}' > ./src/main.rs
COPY ["Cargo.toml", "Cargo.lock",  "./"]
RUN cargo build --release
# end - build dependency cache

COPY ./ .

# need to break the cargo cache
RUN touch -a -m ./src/main.rs
RUN cargo build --release  --features embedded-db

##############################
## Create image
##############################
FROM ubuntu:22.04

RUN apt-get update && \
  apt-get install -y ca-certificates && \
  apt-get clean

WORKDIR /ebills

# Copy essential build files
COPY --from=rust-builder /ebills/target/release/bitcredit ./bitcredit
COPY --from=rust-builder /ebills/frontend ./frontend

# Create additional directories and set user permissions
RUN mkdir data

ENV ROCKET_ADDRESS=0.0.0.0

# Expose web server port
EXPOSE 8000

# Expose P2P port
EXPOSE 1908

CMD ["/ebills/bitcredit"]
