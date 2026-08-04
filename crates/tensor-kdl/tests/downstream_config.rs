use tensor_kdl::{
    CtxResult, Decode, DecodeScalar, ErrorCode, ErrorCtx, Located, Opts, Value, read,
    read_into_with_opts, read_nodes_into_visit,
};

#[derive(Debug, Default, Decode, PartialEq)]
struct Config {
    #[kdl(child, unwrap(argument))]
    layout: Option<String>,
}

#[test]
fn typed_document_roots_reject_unknown_nodes_by_default() {
    let error = read::<Config>("compatibility #true").unwrap_err();
    assert_eq!(error.code, ErrorCode::UnknownChild);
    assert_eq!(error.consumed, 0, "diagnostic points at the unknown name");
}

#[test]
fn typed_document_roots_can_explicitly_skip_unknown_nodes() {
    let mut config = Config::default();
    let error = read_into_with_opts(
        &mut config,
        "compatibility #true\nlayout \"scrolling-1d\"",
        &mut tensor_kdl::Context::new(),
        Opts::lenient(),
    );
    assert!(error.is_ok(), "{error}");
    assert_eq!(config.layout.as_deref(), Some("scrolling-1d"));
}

#[test]
fn repeated_known_nodes_keep_first_wins_semantics_in_strict_mode() {
    let config = read::<Config>("layout \"scrolling-1d\"\nlayout \"spatial-2d\"").unwrap();
    assert_eq!(config.layout.as_deref(), Some("scrolling-1d"));
}

#[derive(Debug, Decode)]
struct LocatedRow {
    #[kdl(property)]
    count: Located<u32>,
}

#[derive(Debug, Decode, Default)]
#[kdl(validate = "validate_kdl")]
struct ExclusiveRow {
    #[kdl(property)]
    proportion: Option<Located<u32>>,
    #[kdl(property)]
    fixed: Option<Located<u32>>,
}

impl ExclusiveRow {
    fn validate_kdl(&self, node_offset: usize) -> CtxResult<()> {
        match (&self.proportion, &self.fixed) {
            (Some(proportion), Some(fixed)) => Err(ErrorCtx::new(
                ErrorCode::DuplicateProperty,
                proportion.offset().max(fixed.offset()),
            )
            .with_message("set either proportion or fixed, not both")),
            (None, None) => Err(ErrorCtx::new(ErrorCode::MissingProperty, node_offset)
                .with_message("requires proportion or fixed")),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Positive(u32);

impl<'a> DecodeScalar<'a> for Positive {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        let value = u32::decode_scalar(value)?;
        if value == 0 {
            return Err(ErrorCtx::new(ErrorCode::ExceededLimit, 0)
                .with_message("value must be greater than zero"));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Decode)]
struct ValidatedRow {
    #[kdl(property)]
    count: Positive,
}

#[derive(Debug, Default, Decode)]
struct LocatedRoot {
    #[kdl(child, unwrap(argument))]
    limit: Option<Located<u32>>,
}

#[test]
fn streaming_decode_retains_argument_and_property_offsets() {
    let property_input = "row count=42";
    let mut rows = Vec::<LocatedRow>::new();
    let error = read_nodes_into_visit(
        &mut rows,
        property_input,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert!(error.is_ok(), "{error}");
    assert_eq!(rows[0].count.value(), &42);
    assert_eq!(
        rows[0].count.offset(),
        property_input.find("count").unwrap()
    );

    let argument_input = "limit 9";
    let root = read::<LocatedRoot>(argument_input).unwrap();
    let limit = root.limit.unwrap();
    assert_eq!(limit.value(), &9);
    assert_eq!(limit.offset(), argument_input.find('9').unwrap());
}

#[test]
fn scalar_validation_and_unknown_properties_point_at_their_entries() {
    let mut valid = Vec::<ValidatedRow>::new();
    let error = read_nodes_into_visit(
        &mut valid,
        "row count=1",
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert!(error.is_ok(), "{error}");
    assert_eq!(valid[0].count, Positive(1));

    let validation_input = "row count=0";
    let error = read_nodes_into_visit(
        &mut Vec::<ValidatedRow>::new(),
        validation_input,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert_eq!(error.code, ErrorCode::ExceededLimit);
    assert_eq!(error.consumed, validation_input.find("count").unwrap());

    let unknown_input = "row count=1 typo=2";
    let error = read_nodes_into_visit(
        &mut Vec::<ValidatedRow>::new(),
        unknown_input,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert_eq!(error.code, ErrorCode::UnknownProperty);
    assert_eq!(error.consumed, unknown_input.find("typo").unwrap());
}

#[test]
fn node_completion_validation_receives_node_and_field_offsets() {
    let valid = "width proportion=1";
    let mut rows = Vec::<ExclusiveRow>::new();
    let error = read_nodes_into_visit(
        &mut rows,
        valid,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert!(error.is_ok(), "{error}");

    let conflict = "width proportion=1 fixed=2";
    let error = read_nodes_into_visit(
        &mut Vec::<ExclusiveRow>::new(),
        conflict,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert_eq!(error.code, ErrorCode::DuplicateProperty);
    assert_eq!(error.consumed, conflict.find("fixed").unwrap());

    let missing = "\n  width";
    let error = read_nodes_into_visit(
        &mut Vec::<ExclusiveRow>::new(),
        missing,
        &mut tensor_kdl::Context::new(),
        Opts::new(),
    );
    assert_eq!(error.code, ErrorCode::MissingProperty);
    assert_eq!(error.consumed, missing.find("width").unwrap());
}

#[cfg(feature = "dom")]
mod dom {
    use tensor_kdl::{Decode, DecodePartial, ErrorCode, Node, Value, read};

    #[derive(Debug, Default)]
    struct Extensions;

    impl<'a> DecodePartial<'a> for Extensions {
        fn insert_child(&mut self, node: &Node<'a>) -> tensor_kdl::CtxResult<bool> {
            Ok(node.name.as_str() == "extension")
        }

        fn insert_property(
            &mut self,
            _key: &str,
            _value: &Value<'a>,
        ) -> tensor_kdl::CtxResult<bool> {
            Ok(false)
        }
    }

    #[derive(Debug, Decode, Default)]
    struct ConfigWithExtensions {
        #[kdl(child, unwrap(argument))]
        layout: Option<String>,
        #[kdl(flatten)]
        _extensions: Extensions,
    }

    #[test]
    fn flatten_roots_reject_nodes_not_consumed_by_the_extension() {
        let error = read::<ConfigWithExtensions>("compatibility #true").unwrap_err();
        assert_eq!(error.code, ErrorCode::UnknownChild);
    }

    #[test]
    fn flatten_roots_accept_nodes_consumed_by_the_extension() {
        let config =
            read::<ConfigWithExtensions>("extension \"example\"\nlayout \"scrolling-1d\"").unwrap();
        assert_eq!(config.layout.as_deref(), Some("scrolling-1d"));
    }
}
