use reqwest::Url;
use serde::{Deserialize, Serialize};

/**
A pair of [`String`] and [`Url`] destined for the list of subscribed channels for [`nix-channel`](https://nixos.org/manual/nix/stable/command-ref/nix-channel.html)
*/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelValue(pub String, pub Url);

#[cfg(feature = "cli")]
impl clap::builder::ValueParserFactory for ChannelValue {
    type Parser = ChannelValueParser;
    fn value_parser() -> Self::Parser {
        ChannelValueParser
    }
}

impl From<(String, Url)> for ChannelValue {
    fn from((string, url): (String, Url)) -> Self {
        Self(string, url)
    }
}

#[derive(Clone, Debug)]
pub struct ChannelValueParser;

#[cfg(feature = "cli")]
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
