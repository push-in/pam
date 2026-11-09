# PAM from a local runtime directory (an unreleased build), used when
# BENCH_PAM_RUNTIME points at an extracted PAM runtime with bin/pam-run.
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY . /opt/pam
RUN ln -s /opt/pam/bin/pam-run /usr/local/bin/pam && pam --version
WORKDIR /app
