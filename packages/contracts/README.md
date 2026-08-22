# pushinbr/pam-contracts

Small, versioned contracts for packages that extend Pam. It contains the HTTP
application and middleware contracts, service providers, stability values and
native runtime compatibility checks. It also defines the protocol-1 provider
contract for queues, pub/sub, streams, and RPC transports. It does not contain a
router, broker client, or server.

```bash
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
