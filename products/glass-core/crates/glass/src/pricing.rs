//! Model pricing: dollars per million tokens.
//!
//! The harness already reasons about cost everywhere — cache hit
//! rates, warmer economics, the printed /btw detour — but until
//! now the numbers were tokens. A rate card turns them into the
//! unit a budget is actually set in.
//!
//! The built-in table covers Anthropic's published list prices,
//! because that is the default provider. Every OpenAI-compatible
//! endpoint prices differently, so those need the `[pricing]`
//! config section, which always wins over the table.

/// Price of one million tokens, in USD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateCard {
    /// Prompt tokens that were neither read from nor written to cache.
    pub input: f64,
    /// Prompt tokens served from an existing cache entry: 0.1x input.
    pub cache_read: f64,
    /// Prompt tokens written to cache this request: 1.25x input.
    pub cache_write: f64,
    pub output: f64,
}

impl RateCard {
    /// Cost of one request, in USD.
    pub fn cost(&self, usage: &crate::provider::Usage) -> f64 {
        (usage.input as f64 * self.input
            + usage.cache_read as f64 * self.cache_read
            + usage.cache_write as f64 * self.cache_write
            + usage.output as f64 * self.output)
            / 1_000_000.0
    }
}

// Anthropic list prices, per million tokens. Opus 5/25, Sonnet 3/15,
// Haiku 1/5, with cache read at 0.1x and cache write at 1.25x input.
const OPUS: RateCard = RateCard {
    input: 5.0,
    cache_read: 0.5,
    cache_write: 6.25,
    output: 25.0,
};
const SONNET: RateCard = RateCard {
    input: 3.0,
    cache_read: 0.3,
    cache_write: 3.75,
    output: 15.0,
};
const HAIKU: RateCard = RateCard {
    input: 1.0,
    cache_read: 0.1,
    cache_write: 1.25,
    output: 5.0,
};

/// Built-in card for a model id, matched by substring.
///
/// Deliberately conservative: an unknown model gets `None`, and the
/// usage line then shows tokens only. A wrong price is worse than no
/// price, because it is the number a budget decision is made from.
pub fn builtin(model: &str) -> Option<RateCard> {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") {
        Some(OPUS)
    } else if m.contains("sonnet") {
        Some(SONNET)
    } else if m.contains("haiku") {
        Some(HAIKU)
    } else {
        None
    }
}

/// Format a dollar amount for the usage line.
///
/// Whole dollars need cents; a single turn of a cheap model can cost
/// less than a cent, so small amounts keep a third decimal rather
/// than rounding to "$0.00".
pub fn money(c: f64) -> String {
    if c >= 1.0 {
        format!("${c:.2}")
    } else {
        format!("${c:.3}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Usage;

    fn u(input: u32, read: u32, write: u32, output: u32) -> Usage {
        Usage {
            input,
            cache_read: read,
            cache_write: write,
            output,
        }
    }

    #[test]
    fn builtin_covers_the_anthropic_families() {
        assert_eq!(builtin("claude-opus-4-5"), Some(OPUS));
        assert_eq!(builtin("claude-sonnet-4-5"), Some(SONNET));
        assert_eq!(builtin("claude-3-5-haiku"), Some(HAIKU));
        // Matching is case-insensitive and substring-based.
        assert_eq!(builtin("Claude-Opus-4-5"), Some(OPUS));
    }

    #[test]
    fn unknown_models_have_no_price() {
        // OpenAI-compatible endpoints price per-deployment, so a guess
        // would be a lie. None means the $ simply does not appear.
        assert_eq!(builtin("gpt-5"), None);
        assert_eq!(builtin("qwen3-coder"), None);
        assert_eq!(builtin(""), None);
    }

    #[test]
    fn cost_sums_every_bucket_at_its_own_rate() {
        let card = RateCard {
            input: 5.0,
            cache_read: 0.5,
            cache_write: 6.25,
            output: 25.0,
        };
        // 1M uncached input + 1M cached read + 1M cache write + 1M output.
        let c = card.cost(&u(1_000_000, 1_000_000, 1_000_000, 1_000_000));
        assert!((c - 36.75).abs() < 1e-9, "{c}");
    }

    #[test]
    fn a_typical_turn_costs_cents() {
        let c = OPUS.cost(&u(12_000, 100_000, 0, 1_500));
        // 12k*5 + 100k*0.5 + 1.5k*25, per million.
        assert!((c - 0.1475).abs() < 1e-9, "{c}");
    }

    #[test]
    fn money_keeps_precision_below_a_dollar() {
        assert_eq!(money(1.5), "$1.50");
        assert_eq!(money(0.0412), "$0.041");
        assert_eq!(money(0.0), "$0.000");
    }
}
