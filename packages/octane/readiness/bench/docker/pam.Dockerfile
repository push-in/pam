FROM ubuntu:24.04
ARG PAM_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl tzdata \
    && rm -rf /var/lib/apt/lists/*
ENV PAM_INSTALL_DIR=/opt/pam PAM_BIN_DIR=/usr/local/bin
RUN curl --proto '=https' --tlsv1.2 -fsSL \
      "https://github.com/push-in/pam/releases/download/v${PAM_VERSION}/install.sh" -o /tmp/install.sh \
    && PAM_VERSION="v${PAM_VERSION}" sh /tmp/install.sh \
    && rm /tmp/install.sh

RUN pam --version
WORKDIR /app
