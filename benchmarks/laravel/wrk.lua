done = function(summary, latency, requests)
  local errors = summary.errors.connect + summary.errors.read + summary.errors.write + summary.errors.status + summary.errors.timeout
  io.write(string.format(
    '{"requests":%d,"duration_us":%d,"bytes":%d,"rps":%.4f,"errors":%d,"latency":{"p50_us":%d,"p75_us":%d,"p90_us":%d,"p95_us":%d,"p99_us":%d,"max_us":%d}}\n',
    summary.requests,
    summary.duration,
    summary.bytes,
    summary.requests / (summary.duration / 1000000),
    errors,
    latency:percentile(50.0),
    latency:percentile(75.0),
    latency:percentile(90.0),
    latency:percentile(95.0),
    latency:percentile(99.0),
    latency.max
  ))
end
