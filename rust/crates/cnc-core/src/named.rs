//! Support for the games' many "type" enums that are identified by a name in INI files.

/// Error returned when parsing an INI name that matches no variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownName {
    /// The type being parsed.
    pub kind: &'static str,
    /// The name that wasn't recognised.
    pub name: String,
}

impl std::fmt::Display for UnknownName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown {} name {:?}", self.kind, self.name)
    }
}

impl std::error::Error for UnknownName {}

/// Defines a fieldless enum whose variants have a fixed numeric id (the original enum
/// value, used in binary formats) and an INI name.
///
/// The generated type gets `ALL`, `id`, `from_id`, `name`, `FromStr` (case-insensitive)
/// and `Display` (the INI name).
///
/// ```
/// cnc_core::named_enum! {
///     /// A theater.
///     pub enum Theater: u8 {
///         /// Temperate.
///         Temperate = 0 => "TEMPERATE",
///         /// Snow.
///         Snow = 1 => "SNOW",
///     }
/// }
/// assert_eq!("snow".parse::<Theater>(), Ok(Theater::Snow));
/// assert_eq!(Theater::from_id(0), Some(Theater::Temperate));
/// assert_eq!(Theater::Snow.to_string(), "SNOW");
/// ```
#[macro_export]
macro_rules! named_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident : $repr:ty {
            $(
                $(#[$vmeta:meta])*
                $variant:ident = $id:literal => $ini:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr($repr)]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant = $id,
            )+
        }

        impl $name {
            /// Every variant, in id order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The numeric id used by the original engine and its binary formats.
            #[must_use]
            pub const fn id(self) -> $repr {
                self as $repr
            }

            /// Looks up a variant by numeric id.
            #[must_use]
            pub const fn from_id(id: $repr) -> Option<Self> {
                match id {
                    $($id => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// The name used in INI files.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $ini,)+
                }
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::named::UnknownName;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let text = text.trim();
                Self::ALL
                    .iter()
                    .copied()
                    .find(|v| v.name().eq_ignore_ascii_case(text))
                    .ok_or_else(|| $crate::named::UnknownName {
                        kind: stringify!($name),
                        name: text.to_owned(),
                    })
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}
