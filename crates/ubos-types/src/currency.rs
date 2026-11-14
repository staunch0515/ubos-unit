//! ISO 4217 currency reference data: the alphabetic code, the minor unit and the dates
//! between which a code is in use (STD-FND-023, FR-LOC-021, DSN-DVM-410).
//!
//! The table is a snapshot of the published list, named by `TABLE_VERSION`; a code that
//! was withdrawn stays in the table with its end date, so that money stored before the
//! withdrawal keeps validating. Fund codes and the codes of precious metals and of
//! testing are not listed.

/// The edition of the table: the date of the ISO 4217 list it follows.
pub const TABLE_VERSION: &str = "2026-01-01";

/// One currency of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Currency {
    /// The alphabetic code.
    pub code: &'static str,
    /// The number of fraction digits of the minor unit.
    pub minor_unit: u32,
    /// The first date of use, when the table records one.
    pub from: Option<&'static str>,
    /// The date from which the code is no longer in use (exclusive end).
    pub to: Option<&'static str>,
}

const fn c(code: &'static str, minor_unit: u32) -> Currency {
    Currency {
        code,
        minor_unit,
        from: None,
        to: None,
    }
}

const fn dated(
    code: &'static str,
    minor_unit: u32,
    from: Option<&'static str>,
    to: Option<&'static str>,
) -> Currency {
    Currency {
        code,
        minor_unit,
        from,
        to,
    }
}

/// The currencies, ordered by code.
pub const CURRENCIES: &[Currency] = &[
    c("AED", 2),
    c("AFN", 2),
    c("ALL", 2),
    c("AMD", 2),
    c("AOA", 2),
    c("ARS", 2),
    c("AUD", 2),
    c("AWG", 2),
    c("AZN", 2),
    c("BAM", 2),
    c("BBD", 2),
    c("BDT", 2),
    dated("BGN", 2, None, Some("2026-01-01")),
    c("BHD", 3),
    c("BIF", 0),
    c("BMD", 2),
    c("BND", 2),
    c("BOB", 2),
    c("BRL", 2),
    c("BSD", 2),
    c("BTN", 2),
    c("BWP", 2),
    c("BYN", 2),
    c("BZD", 2),
    c("CAD", 2),
    c("CDF", 2),
    c("CHF", 2),
    c("CLP", 0),
    c("CNY", 2),
    c("COP", 2),
    c("CRC", 2),
    c("CUP", 2),
    c("CVE", 2),
    c("CZK", 2),
    c("DJF", 0),
    c("DKK", 2),
    c("DOP", 2),
    c("DZD", 2),
    c("EGP", 2),
    c("ERN", 2),
    c("ETB", 2),
    c("EUR", 2),
    c("FJD", 2),
    c("FKP", 2),
    c("GBP", 2),
    c("GEL", 2),
    c("GHS", 2),
    c("GIP", 2),
    c("GMD", 2),
    c("GNF", 0),
    c("GTQ", 2),
    c("GYD", 2),
    c("HKD", 2),
    c("HNL", 2),
    dated("HRK", 2, None, Some("2023-01-01")),
    c("HTG", 2),
    c("HUF", 2),
    c("IDR", 2),
    c("ILS", 2),
    c("INR", 2),
    c("IQD", 3),
    c("IRR", 2),
    c("ISK", 0),
    c("JMD", 2),
    c("JOD", 3),
    c("JPY", 0),
    c("KES", 2),
    c("KGS", 2),
    c("KHR", 2),
    c("KMF", 0),
    c("KPW", 2),
    c("KRW", 0),
    c("KWD", 3),
    c("KYD", 2),
    c("KZT", 2),
    c("LAK", 2),
    c("LBP", 2),
    c("LKR", 2),
    c("LRD", 2),
    c("LSL", 2),
    c("LYD", 3),
    c("MAD", 2),
    c("MDL", 2),
    c("MGA", 2),
    c("MKD", 2),
    c("MMK", 2),
    c("MNT", 2),
    c("MOP", 2),
    c("MRU", 2),
    c("MUR", 2),
    c("MVR", 2),
    c("MWK", 2),
    c("MXN", 2),
    c("MYR", 2),
    c("MZN", 2),
    c("NAD", 2),
    c("NGN", 2),
    c("NIO", 2),
    c("NOK", 2),
    c("NPR", 2),
    c("NZD", 2),
    c("OMR", 3),
    c("PAB", 2),
    c("PEN", 2),
    c("PGK", 2),
    c("PHP", 2),
    c("PKR", 2),
    c("PLN", 2),
    c("PYG", 0),
    c("QAR", 2),
    c("RON", 2),
    c("RSD", 2),
    c("RUB", 2),
    c("RWF", 0),
    c("SAR", 2),
    c("SBD", 2),
    c("SCR", 2),
    c("SDG", 2),
    c("SEK", 2),
    c("SGD", 2),
    c("SHP", 2),
    dated("SLE", 2, Some("2022-07-01"), None),
    c("SOS", 2),
    c("SRD", 2),
    c("SSP", 2),
    c("STN", 2),
    c("SVC", 2),
    c("SYP", 2),
    c("SZL", 2),
    c("THB", 2),
    c("TJS", 2),
    c("TMT", 2),
    c("TND", 3),
    c("TOP", 2),
    c("TRY", 2),
    c("TTD", 2),
    c("TWD", 2),
    c("TZS", 2),
    c("UAH", 2),
    c("UGX", 0),
    c("USD", 2),
    c("UYU", 2),
    c("UZS", 2),
    c("VES", 2),
    c("VND", 0),
    c("VUV", 0),
    c("WST", 2),
    c("XAF", 0),
    c("XCD", 2),
    c("XOF", 0),
    c("XPF", 0),
    c("YER", 2),
    c("ZAR", 2),
    c("ZMW", 2),
];

/// The currency of a code, withdrawn codes included.
pub fn currency(code: &str) -> Option<&'static Currency> {
    CURRENCIES
        .binary_search_by(|x| x.code.cmp(code))
        .ok()
        .map(|i| &CURRENCIES[i])
}

impl Currency {
    /// True when the code is in use on a date `YYYY-MM-DD`.
    pub fn in_use(&self, date: &str) -> bool {
        self.from.is_none_or(|f| f <= date) && self.to.is_none_or(|t| date < t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_ordered_and_unique() {
        for w in CURRENCIES.windows(2) {
            assert!(w[0].code < w[1].code, "{} before {}", w[0].code, w[1].code);
        }
        for x in CURRENCIES {
            assert_eq!(x.code.len(), 3);
            assert!(x.code.bytes().all(|b| b.is_ascii_uppercase()));
            assert!(x.minor_unit <= 3);
        }
    }

    #[test]
    fn lookups() {
        assert_eq!(currency("JPY").map(|x| x.minor_unit), Some(0));
        assert_eq!(currency("KWD").map(|x| x.minor_unit), Some(3));
        assert_eq!(currency("USD").map(|x| x.minor_unit), Some(2));
        assert!(currency("XYZ").is_none());
        let hrk = currency("HRK").expect("listed");
        assert!(hrk.in_use("2022-12-31") && !hrk.in_use("2023-01-01"));
    }
}
