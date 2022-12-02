use anyhow::Result;
use fiberplane::base64uuid::Base64Uuid;
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

fn default_base_url() -> Url {
    // This cannot panic since we give it a fixed valid input.
    Url::parse("https://studio.fiberplane.com/").unwrap()
}

pub struct NotebookUrlBuilder {
    workspace_id: Base64Uuid,
    notebook_id: Base64Uuid,

    base_url: Option<Url>,
    title: Option<String>,
    cell_id: Option<String>,
}

impl NotebookUrlBuilder {
    pub fn new(workspace_id: impl Into<Base64Uuid>, notebook_id: impl Into<Base64Uuid>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            notebook_id: notebook_id.into(),
            base_url: None,
            title: None,
            cell_id: None,
        }
    }

    /// Override the default base url
    pub fn base_url(mut self, base_url: impl Into<Url>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Include a sluggified version of the title in the url
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add a deeplink to the cell id in the url
    pub fn cell_id(mut self, cell_id: impl Into<String>) -> Self {
        self.cell_id = Some(cell_id.into());
        self
    }

    /// Build the URL to the notebook
    pub fn url(self) -> Result<Url> {
        let mut u = self.base_url.unwrap_or_else(default_base_url);

        let notebook_slug = match self.title {
            Some(title) => format!("{}-{}", slugify(title), self.notebook_id),
            None => self.notebook_id.to_string(),
        };

        u.path_segments_mut().unwrap().extend(&[
            "workspaces",
            &self.workspace_id.to_string(),
            "notebooks",
            &notebook_slug,
        ]);

        if let Some(cell_id) = self.cell_id {
            u.set_fragment(Some(&cell_id));
        }

        Ok(u)
    }
}

static TICK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"'").unwrap());
static NON_WORD_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\W").unwrap());
static DASH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"-{2,}").unwrap());

/// Create a URL safe slug from input.
///
/// This is based on the following typescript implementation:
/// https://github.com/fiberplane/studio/blob/10cb63c9d17f9367447ad1898a41d9eace96be64/src/utils/createNotebookLink.ts
fn slugify(input: String) -> String {
    let input = TICK_REGEX.replace_all(&input, "");
    let input = NON_WORD_REGEX.replace_all(&input, "-");
    let input = DASH_REGEX.replace_all(&input, "-");

    input.into_owned()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn notebook_url_builder_everything() {
        let url = NotebookUrlBuilder::new(
            Base64Uuid::parse_str("JwpDrHrlS-OWjxYXe9gJ2g").unwrap(),
            Base64Uuid::parse_str("ftTv2S3yRPyJyQQbopXonQ").unwrap(),
        )
        .base_url(url::Url::parse("https://dev.fiberplane.io").unwrap())
        .title("Reported issues on API")
        .cell_id(Base64Uuid::parse_str("dNJvBmg90N-dR_6iZV99LQ").unwrap())
        .url()
        .unwrap();

        assert_eq!(
            url.as_str(),
            "https://dev.fiberplane.io/workspaces/JwpDrHrlS-OWjxYXe9gJ2g/notebooks/Reported-issues-on-API-ftTv2S3yRPyJyQQbopXonQ#dNJvBmg90N-dR_6iZV99LQ"
        );
    }

    #[test]
    fn notebook_url_builder_minimum() {
        let url = NotebookUrlBuilder::new(
            Base64Uuid::parse_str("JwpDrHrlS-OWjxYXe9gJ2g").unwrap(),
            Base64Uuid::parse_str("ftTv2S3yRPyJyQQbopXonQ").unwrap(),
        )
        .url()
        .unwrap();

        assert_eq!(
            url.as_str(),
            "https://studio.fiberplane.com/workspaces/JwpDrHrlS-OWjxYXe9gJ2g/notebooks/ftTv2S3yRPyJyQQbopXonQ"
        );
    }

    #[test]
    fn slugify_test() {
        let tests = vec![
            ("title", "title"),
            ("title   title", "title-title"),
            ("title 😁 title", "title-title"),
            ("title---title", "title-title"),
            ("title-----title", "title-title"),
        ];

        for test in tests {
            let (input, expected) = test;
            let actual = slugify(input.to_string());
            assert_eq!(actual, expected.to_string());
        }
    }
}
