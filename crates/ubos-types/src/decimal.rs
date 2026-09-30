//! Exact decimals with canonical text and the rounding modes of STD-FND-022.

use crate::error::{BpaError, invalid};
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use std::cmp::Ordering;

/// An exact decimal `mantissa / 10^scale`.
#[derive(Debug, Clone)]
pub struct Decimal {
    mantissa: BigInt,
    scale: u32,
}

/// Rounding modes of STD-FND-022.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    /// Round half to even.
    HalfEven,
    /// Round half away from zero.
    HalfUp,
    /// Toward zero.
    Down,
    /// Away from zero.
    Up,
    /// Toward positive infinity.
    Ceiling,
    /// Toward negative infinity.
    Floor,
}

impl Rounding {
    /// Parse a rounding-mode name.
    pub fn parse(name: &str) -> Result<Rounding, BpaError> {
        Ok(match name {
            "half_even" => Rounding::HalfEven,
            "half_up" => Rounding::HalfUp,
            "down" => Rounding::Down,
            "up" => Rounding::Up,
            "ceiling" => Rounding::Ceiling,
            "floor" => Rounding::Floor,
            _ => return Err(invalid("unknown rounding mode")),
        })
    }
}

fn pow10(n: u32) -> BigInt {
    BigInt::from(10u32).pow(n)
}

/// `n / d` rounded to an integer with the given mode (d > 0 after sign handling).
fn round_div(n: &BigInt, d: &BigInt, mode: Rounding) -> BigInt {
    let negative = (n.sign() == Sign::Minus) != (d.sign() == Sign::Minus) && !n.is_zero();
    let (q, r) = n.abs().div_rem(&d.abs());
    if r.is_zero() {
        return if negative { -q } else { q };
    }
    let twice: BigInt = &r * 2;
    let half = twice.cmp(&d.abs());
    let up = match mode {
        Rounding::Down => false,
        Rounding::Up => true,
        Rounding::Ceiling => !negative,
        Rounding::Floor => negative,
        Rounding::HalfUp => half != Ordering::Less,
        Rounding::HalfEven => half == Ordering::Greater || (half == Ordering::Equal && q.is_odd()),
    };
    let q = if up { q + BigInt::one() } else { q };
    if negative { -q } else { q }
}

impl Decimal {
    /// Parse a decimal string of the grammar of STD-FND-022.
    pub fn parse(text: &str) -> Result<Decimal, BpaError> {
        let body = text.strip_prefix('-').unwrap_or(text);
        let (int, frac) = match body.split_once('.') {
            Some((i, f)) => (i, Some(f)),
            None => (body, None),
        };
        let int_ok = int == "0"
            || (!int.is_empty()
                && !int.starts_with('0')
                && int.bytes().all(|b| b.is_ascii_digit()));
        let frac_ok = frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()));
        if !int_ok || !frac_ok {
            return Err(invalid("not a decimal string"));
        }
        let digits = format!("{}{}", int, frac.unwrap_or(""));
        let mut mantissa: BigInt = digits
            .parse()
            .map_err(|_| invalid("not a decimal string"))?;
        if text.starts_with('-') {
            mantissa = -mantissa;
        }
        Ok(Decimal {
            mantissa,
            scale: frac.map_or(0, |f| f.len() as u32),
        })
    }

    /// Number of fraction digits as written.
    pub fn scale(&self) -> u32 {
        self.scale
    }

    /// Number of significant digits of the mantissa as written.
    pub fn digits(&self) -> usize {
        self.mantissa.abs().to_string().len()
    }

    fn at_scale(&self, scale: u32) -> BigInt {
        &self.mantissa * pow10(scale - self.scale)
    }

    /// Round to `scale` fraction digits.
    pub fn round(&self, scale: u32, mode: Rounding) -> Decimal {
        if self.scale <= scale {
            return Decimal {
                mantissa: self.at_scale(scale),
                scale,
            };
        }
        Decimal {
            mantissa: round_div(&self.mantissa, &pow10(self.scale - scale), mode),
            scale,
        }
    }

    /// Exact sum.
    pub fn add(&self, other: &Decimal) -> Decimal {
        let s = self.scale.max(other.scale);
        Decimal {
            mantissa: self.at_scale(s) + other.at_scale(s),
            scale: s,
        }
    }

    /// Exact difference.
    pub fn sub(&self, other: &Decimal) -> Decimal {
        let s = self.scale.max(other.scale);
        Decimal {
            mantissa: self.at_scale(s) - other.at_scale(s),
            scale: s,
        }
    }

    /// Exact product.
    pub fn mul(&self, other: &Decimal) -> Decimal {
        Decimal {
            mantissa: &self.mantissa * &other.mantissa,
            scale: self.scale + other.scale,
        }
    }

    /// Quotient rounded to `scale` digits.
    pub fn div(&self, other: &Decimal, scale: u32, mode: Rounding) -> Result<Decimal, BpaError> {
        if other.mantissa.is_zero() {
            return Err(BpaError::new("LOGIC.DIVISION_BY_ZERO", "division by zero"));
        }
        let n = &self.mantissa * pow10(other.scale + scale);
        let d = &other.mantissa * pow10(self.scale);
        Ok(Decimal {
            mantissa: round_div(&n, &d, mode),
            scale,
        })
    }

    /// Negation.
    pub fn neg(&self) -> Decimal {
        Decimal {
            mantissa: -self.mantissa.clone(),
            scale: self.scale,
        }
    }

    /// True when the value is zero.
    pub fn is_zero(&self) -> bool {
        self.mantissa.is_zero()
    }

    /// Canonical text: exactly `scale` fraction digits when given, otherwise no trailing
    /// fraction zeros; never a negative zero (STD-FND-022).
    pub fn format(&self, scale: Option<u32>) -> String {
        let (m, s) = match scale {
            Some(k) => {
                let r = self.round(k, Rounding::HalfEven);
                (r.mantissa, k)
            }
            None => {
                let mut m = self.mantissa.clone();
                let mut s = self.scale;
                while s > 0 && (&m % 10u32).is_zero() {
                    m /= 10u32;
                    s -= 1;
                }
                (m, s)
            }
        };
        let negative = m.is_negative();
        let digits = m.abs().to_string();
        let digits = if digits.len() <= s as usize {
            format!("{}{}", "0".repeat(s as usize + 1 - digits.len()), digits)
        } else {
            digits
        };
        let split = digits.len() - s as usize;
        let mut out = String::new();
        if negative {
            out.push('-');
        }
        out.push_str(&digits[..split]);
        if s > 0 {
            out.push('.');
            out.push_str(&digits[split..]);
        }
        out
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let s = self.scale.max(other.scale);
        self.at_scale(s).cmp(&other.at_scale(s))
    }
}
