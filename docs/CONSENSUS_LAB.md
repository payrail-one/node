# Seven-validator consensus lab

Run the reproducible scenario report from the `platform` directory:

```text
cargo run -p consensus-lab
```

The default topology has seven equal-weight validators across three failure
domains (3/2/2), 2 ms directed latency inside a domain and 20 ms across domains.
It reports quorum latency for the healthy case, verifies that losing a two-node
domain preserves liveness, and verifies that only four reachable validators
cannot finalize.

Tests additionally cover a 5/2 partition, catch-up after healing, a 4/3
partition and asymmetric message loss. Successful rounds use actual Ed25519
certificates and the reference finality verifier. Network timing is modeled, so
these numbers must never be presented as production latency or throughput.
