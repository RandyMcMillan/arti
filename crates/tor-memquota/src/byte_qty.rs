//! `ByteQty`, Quantity of memory used, measured in bytes.
//
// The closest crate to this on crates.io is `bytesize`.
// But it has serious bugs including confusion about KiB vs KB,
// and isn't maintained.
//
// There is also humansize, but that just does printing.

use crate::internal_prelude::*;

/// Quantity of memory used, measured in bytes.
///
/// Like `usize` but `Display`s in a more friendly and less precise way
#[derive(Debug, Clone, Copy, Hash, Default, Eq, PartialEq, Ord, PartialOrd)] //
#[derive(From, Into, Deref, DerefMut, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Qty(pub(crate) usize);

impl Qty {
    /// Maximum for the type
    pub(crate) const MAX: Qty = Qty(usize::MAX);

    /// Return the value as a plain number, a `usize`
    ///
    /// Provided so call sites don't need to write an opaque `.0` everywhere,
    /// even though that would be fine.
    pub(crate) const fn as_usize(self) -> usize {
        self.0
    }
}

impl Display for Qty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mb = self.0 as f32 / (1024. * 1024.);
        write!(f, "{:.2}MiB", mb)
    }
}

#[cfg(test)]
mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_duration_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->

    use super::*;

    #[test]
    fn display_qty() {
        let chk = |by, s| assert_eq!(Qty(by).to_string(), s);

        chk(10 * 1024, "0.01MiB");
        chk(1024 * 1024, "1.00MiB");
        chk(1000 * 1024 * 1024, "1000.00MiB");
    }
}
