#[derive(Clone, Copy, Debug)]
pub(super) struct ScaleValue(f64);

impl ScaleValue {
    pub(super) const fn get(self) -> f64 {
        self.0
    }
}

impl<S: knus::traits::ErrorSpan> knus::traits::DecodeScalar<S> for ScaleValue {
    fn type_check(
        type_name: &Option<knus::span::Spanned<knus::ast::TypeName, S>>,
        context: &mut knus::decode::Context<S>,
    ) {
        <f64 as knus::traits::DecodeScalar<S>>::type_check(type_name, context);
    }

    fn raw_decode(
        value: &knus::span::Spanned<knus::ast::Literal, S>,
        _context: &mut knus::decode::Context<S>,
    ) -> Result<Self, knus::errors::DecodeError<S>> {
        match &**value {
            knus::ast::Literal::Decimal(decimal) => f64::try_from(decimal)
                .map(Self)
                .map_err(|error| knus::errors::DecodeError::conversion(value, error)),
            knus::ast::Literal::Int(integer) => i64::try_from(integer)
                .map(|value| Self(value as f64))
                .map_err(|error| knus::errors::DecodeError::conversion(value, error)),
            _ => Err(knus::errors::DecodeError::scalar_kind(
                knus::decode::Kind::Decimal,
                value,
            )),
        }
    }
}
