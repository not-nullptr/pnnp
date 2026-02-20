use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonochromeError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("non-200 response: {0}")]
    Non200(String),

    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("unsupported manifest mime type: {0}")]
    UnsupportedManifestMimeType(String),

    #[error("failed to decode manifest")]
    ManifestDecode,

    #[error("manifest error: {0}")]
    Manifest(#[from] MonochromeManifestError),
}

#[derive(Debug, Error)]
pub enum MonochromeManifestError {
    #[error("failed to parse xml: {0}")]
    XmlParse(#[from] roxmltree::Error),

    #[error("missing SegmentTemplate in MPD")]
    MissingSegmentTemplate,

    #[error("missing initialization template in SegmentTemplate")]
    MissingInitializationTemplate,

    #[error("S missing d attribute in SegmentTimeline")]
    SMissingD,

    #[error("missing media")]
    MissingMedia,

    #[error("failed to parse url: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("failed to fetch segment: {0}")]
    FetchSegment(#[from] reqwest::Error),

    #[error("no Representation found")]
    MissingRepresentation,

    #[error("invalid MPD: no MPD node")]
    InvalidMpd,

    #[error("negative repeat values are unsupported")]
    NegativeRepeatUnsupported,

    #[error("failed to parse base URL")]
    BadBaseUrl,

    #[error("fs error: {0}")]
    Fs(#[from] std::io::Error),
}
