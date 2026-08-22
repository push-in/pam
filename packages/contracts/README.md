# pushinbr/pam-contracts

Small, versioned contracts for packages that extend Pam. It contains the HTTP
application and middleware contracts, service providers, stability values and
native runtime compatibility checks. It also defines the protocol-1 provider
contract for queues, pub/sub, streams, and RPC transports. It does not contain a
router, broker client, or server.

## Start here

PAM Contracts is a Composer package for the PAM Runtime; it is not a
standalone runtime. Install PAM first, open your application directory, and
add the package through PAM's Composer toolchain:

```bash
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --connect-timeout 15 --max-time 60 --max-filesize 1048576 -fsSL \
    https://github.com/push-in/pam/releases/latest/download/install.sh | sh

pam doctor
cd my-app
pam composer require pushinbr/pam-contracts
```

Transport packages implement `TransportProviderInterface` and expose a strict
`TransportDescriptor`. `TransportWorker` owns bounded batching, payload checks,
cancellation, acknowledgement/retry/reject decisions, lifecycle cleanup, and
integer-coded observations. Applications register providers through the
optional `TransportApplicationInterface`, preserving compatibility with HTTP-only
implementations.

See PAM's [package model](https://github.com/push-in/pam/blob/main/docs/packages.md).

## License

Free and open-source under the [Apache License 2.0](LICENSE). You may use,
modify, and distribute this package for any purpose, including commercially.
