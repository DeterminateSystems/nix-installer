use reqwest::Url;

#[derive(Debug, Clone)]
pub struct ChannelValue(pub String, pub Url);

impl clap::builder::ValueParserFactory for ChannelValue {
    type Parser = ChannelValueParser;
    fn value_parser() -> Self::Parser {
        ChannelValueParser
    }
}

#[derive(Clone, Debug)]
pub struct ChannelValueParser;
impl clap::builder::TypedValueParser for ChannelValueParser {
    type Value = ChannelValue;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let buf = value.to_str().ok_or_else(|| {
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, "Should be all UTF-8")
        })?;
        let (name, url) = buf.split_once('=').ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidValue,
                "Should be formatted `name=url`",
            )
        })?;
        let name = name.to_owned();
        let url = url.parse().unwrap();
        Ok(ChannelValue(name, url))
    }
}
