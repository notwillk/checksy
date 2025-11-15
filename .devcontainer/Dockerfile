FROM mcr.microsoft.com/devcontainers/base:ubuntu

ARG USERNAME=vscode
ARG USER_UID=1001
ARG USER_GID=$USER_UID

ENV SHELL=/bin/bash

# Install common dependencies
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    apt-transport-https \
    bash \
    ca-certificates \
    curl \
    entr \
    git \
    gnupg \
    just \
    lsb-release \
    ssh \
    sudo \
    yq \
  && rm -rf /var/lib/apt/lists/*

# Copy configuration files
COPY *.yaml /tmp

# Write system-wide aliases from /tmp/aliases.yaml
RUN { \
      echo '# generated aliases'; \
      echo 'case "$-" in *i*) ;; *) return ;; esac'; \
      echo '[ -n "${BASH_VERSION-}${ZSH_VERSION-}" ] || return 0'; \
      yq -r 'to_entries[] | "alias \(.key)='\''\(.value|tostring)'\''"' /tmp/aliases.yaml; \
    } > /etc/profile.d/10-aliases.sh \
  && chmod 0644 /etc/profile.d/10-aliases.sh \
  && grep -q '/etc/profile.d' /etc/bash.bashrc \
    || echo '[ -d /etc/profile.d ] && for f in /etc/profile.d/*.sh; do . "$f"; done' >> /etc/bash.bashrc

# Install Go
RUN set -eu \
  && GO_VERSION="$(yq -r '.golang' /tmp/versions.yaml)" \
  && ARCH="$(dpkg --print-architecture)" \
  && case "$ARCH" in amd64|arm64) GOARCH="$ARCH" ;; *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;; esac \
  && curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${GOARCH}.tar.gz" -o /tmp/go.tar.gz \
  && rm -rf /usr/local/go \
  && tar -C /usr/local -xzf /tmp/go.tar.gz \
  && ln -sf /usr/local/go/bin/go /usr/local/bin/go \
  && rm /tmp/go.tar.gz \
  && go version

# Install goreleaser and put it on the global PATH for every user
RUN GOBIN=/usr/local/bin go install github.com/goreleaser/goreleaser/v2@latest
