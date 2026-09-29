use std::cmp::Ordering;
use std::sync::LazyLock;

use icu_collator::{Collator, CollatorBorrowed, options::CollatorOptions};
use icu_locale_core::Locale;

static COLLATOR: LazyLock<CollatorBorrowed<'static>> = LazyLock::new(|| {
    // ICU's process locale precedence, also used by Node's Intl/localeCompare.
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok());
    collator(locale.as_deref())
});

pub(super) fn compare(left: &str, right: &str) -> Ordering {
    COLLATOR.compare(left, right)
}

fn collator(name: Option<&str>) -> CollatorBorrowed<'static> {
    let name = name
        .unwrap_or("en-US")
        .split(['.', '@'])
        .next()
        .unwrap_or_default();
    let name = if matches!(name, "C" | "POSIX") {
        "en-US"
    } else {
        name
    };
    let locale = name
        .replace('_', "-")
        .parse::<Locale>()
        .unwrap_or(Locale::UNKNOWN);
    Collator::try_new(locale.into(), CollatorOptions::default())
        .expect("compiled ICU data includes root collation and locale fallback")
}

#[cfg(test)]
mod tests;
