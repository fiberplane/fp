use deno_ast::{MediaType, ParseParams, SourceTextInfo};
use deno_core::{
    futures::FutureExt, resolve_import, ModuleLoadResponse, ModuleLoader, ModuleSource,
    ModuleSourceCode,
};

pub struct TsModuleLoader;

impl ModuleLoader for TsModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: deno_core::ResolutionKind,
    ) -> Result<deno_core::ModuleSpecifier, anyhow::Error> {
        Ok(resolve_import(specifier, referrer)?)
    }

    fn load(
        &self,
        module_specifier: &deno_core::ModuleSpecifier,
        _maybe_referrer: Option<&deno_core::ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: deno_core::RequestedModuleType,
    ) -> ModuleLoadResponse {
        let module_specifier = module_specifier.clone();

        let res = async move {
            let code = match module_specifier.scheme() {
                "http" | "https" => {
                    reqwest::get(module_specifier.as_str())
                        .await?
                        .text()
                        .await?
                }
                "file" => {
                    let path = module_specifier
                        .to_file_path()
                        .expect("Failed to convert to path");
                    std::fs::read_to_string(&path).expect("Failed to read file")
                }
                schema => {
                    anyhow::bail!("Unsupported schema: {}", schema)
                }
            };

            let media_type = MediaType::from_specifier(&module_specifier);
            let (module_type, should_transpile) = match media_type {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (deno_core::ModuleType::JavaScript, false)
                }
                MediaType::Jsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::TypeScript
                | MediaType::Mts
                | MediaType::Cts
                | MediaType::Dts
                | MediaType::Dmts
                | MediaType::Dcts
                | MediaType::Tsx => (deno_core::ModuleType::JavaScript, true),
                MediaType::Json => (deno_core::ModuleType::Json, false),
                _ => panic!("Unknown file media type {:?}", &module_specifier),
            };

            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.clone(),
                    text_info: SourceTextInfo::from_string(code),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                match parsed.transpile(&Default::default(), &Default::default())? {
                    deno_ast::TranspileResult::Cloned(res)
                    | deno_ast::TranspileResult::Owned(res) => res.text,
                }
            } else {
                code
            };

            Ok(ModuleSource::new(
                module_type,
                ModuleSourceCode::String(code.into()),
                &module_specifier,
                None,
            ))
        }
        .boxed_local();

        ModuleLoadResponse::Async(res)
    }
}
