export function summarizeByteSamples(samples) {
  if (samples.length === 0) {
    return { median_bytes: null, p95_bytes: null, samples: 0 };
  }
  const sorted = [...samples].sort((left, right) => left - right);
  return {
    median_bytes: sorted[Math.round((sorted.length - 1) * 0.5)],
    p95_bytes: sorted[Math.round((sorted.length - 1) * 0.95)],
    samples: sorted.length,
  };
}
