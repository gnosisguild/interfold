ARG RUST_VERSION=1.91.1
ARG RISC0_VERSION=3.0.3
ARG RISC0_TOOLCHAIN=1.91.1
ARG SKIP_SOLIDITY=0
FROM rust:${RUST_VERSION}-slim-bookworm AS base-dev

ENV CARGO_HOME=/usr/local/cargo
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    pkg-config \
    git \
    libssl-dev \
    curl \
    sudo \
    ca-certificates \
    protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*
    
# Create user
ENV USERNAME=devuser
ARG USER_UID=1000
ARG USER_GID=1000

RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
    && chmod 0440 /etc/sudoers.d/$USERNAME

USER $USERNAME
WORKDIR /home/$USERNAME

ENV SHELL=/bin/bash

ENV PATH="/home/${USERNAME}/.risc0/bin:${PATH}"
RUN curl -L https://risczero.com/install | bash

# Re-declared inside the stage. An ARG before the first FROM is only in scope for FROM lines, so
# these expanded to nothing here and `rzup install` silently took its own default — the opposite of
# what pinning them is for. A bare ARG picks up the default declared at the top of the file.
ARG RISC0_VERSION
ARG RISC0_TOOLCHAIN

# Empty is the failure this guards against, and it is silent: `rzup install rust` with no version
# installs whatever rzup considers current, and the image still builds.
RUN set -eu; \
    : "${RISC0_TOOLCHAIN:?RISC0_TOOLCHAIN is empty; declare the ARG inside this stage}"; \
    : "${RISC0_VERSION:?RISC0_VERSION is empty; declare the ARG inside this stage}"

RUN rzup install rust ${RISC0_TOOLCHAIN}
RUN rzup install r0vm ${RISC0_VERSION}
RUN rzup install cargo-risczero ${RISC0_VERSION}

RUN sudo ln -sf "$(readlink -f "$(command -v r0vm)")" /usr/local/bin/r0vm && \
    test -x /usr/local/bin/r0vm
ENV RISC0_SERVER_PATH=/usr/local/bin/r0vm

ENV PATH="/home/${USERNAME}/.foundry/bin:${PATH}"
RUN curl -L https://foundry.paradigm.xyz | bash
RUN foundryup

COPY . /app

RUN sudo chown -R $USERNAME:$USERNAME /app

WORKDIR /app

CMD ["/bin/bash"]
