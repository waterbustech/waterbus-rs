#[macro_export]
macro_rules! impl_from_i16_with_default {
    ($enum_name:ident { $first_variant:ident = $first_value:expr, $($variant:ident = $value:expr),+ $(,)? }) => {
        impl From<i16> for $enum_name {
            fn from(value: i16) -> Self {
                match value {
                    $first_value => $enum_name::$first_variant,
                    $( $value => $enum_name::$variant, )+
                    _ => $enum_name::$first_variant,
                }
            }
        }
        impl From<$enum_name> for i16 {
            fn from(value: $enum_name) -> i16 {
                value as i16
            }
        }
    };
}
