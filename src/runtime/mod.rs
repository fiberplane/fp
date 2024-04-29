use std::convert::TryFrom;
use deno_ast::ModuleSpecifier;
use deno_core::serde_v8::from_v8;
use deno_core::{
    v8::{self},
    JsRuntime, PollEventLoopOptions, RuntimeOptions,
};
use fiberplane::models::notebooks::NewNotebook;
use module_loader::TsModuleLoader;
use std::rc::Rc;

mod module_loader;

pub struct EvalRuntime {
    js: JsRuntime,
}

impl EvalRuntime {
    pub fn new() -> Self {
        let js = JsRuntime::new(RuntimeOptions {
            extensions: vec![],
            module_loader: Some(Rc::new(TsModuleLoader)),
            ..Default::default()
        });

        return Self { js };
    }

    pub fn process() {
        todo!()
        // TODO: implement the runtime as a process that can run outside of the main thread
        // and communicate with it using the channels (kinda like the slack service)
    }

    pub async fn evaluate(
        &mut self,
        template: String,
        params: Option<serde_json::Value>,
    ) -> Result<NewNotebook, anyhow::Error> {
        let mut template_path = std::env::current_dir()?;
        template_path.push("template.ts");

        let template_url =
            ModuleSpecifier::from_file_path(template_path).expect("failed to create a file url");

        let template_module = self
            .js
            .load_main_es_module_from_code(&template_url, template)
            .await?;

        let _ = self.js.mod_evaluate(template_module).await?;

        let _ = self.js.run_event_loop(PollEventLoopOptions::default());

        let template_module = self.js.get_module_namespace(template_module)?;
        let template_scope = &mut self.js.handle_scope();
        let template_module = v8::Local::<v8::Object>::new(template_scope, template_module);

        let template_key =
            v8::String::new(template_scope, "default").expect("Couldn't create a v8 string block");
        let template_function = template_module
            .get(template_scope, template_key.into())
            .expect("Couldn't find a default export");
        let template_fn = v8::Local::<v8::Function>::try_from(template_function)?;

        let params = if let Some(params) = params {
            deno_core::serde_v8::to_v8(template_scope, params)?
        } else {
            v8::undefined(template_scope).into()
        };

        let undefined = v8::undefined(template_scope);

        let Some(res) = template_fn.call(template_scope, undefined.into(), &[params]) else {
            anyhow::bail!("Failed to create a notebook from template")
        };

        Ok(from_v8(template_scope, res)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[tokio::test]
    async fn can_execute_ts_template() -> anyhow::Result<()> {
        let mut runtime = EvalRuntime::new();
        let template = std::fs::read_to_string("src/fixtures/example-template.ts")?;
        let result = runtime.evaluate(template.to_string(), None).await?;

        assert_eq!(result.title, "NewNotebook");

        Ok(())
    }

    #[tokio::test]
    async fn can_execute_ts_template_with_params() -> anyhow::Result<()> {
        let mut runtime = EvalRuntime::new();
        let template = std::fs::read_to_string("src/fixtures/example-template-with-params.ts")?;
        let params: serde_json::Value = serde_json::json!({
            "name": "ParamsNotebook",
            "message": "Hi with params"
        });

        let result = runtime.evaluate(template.to_string(), Some(params)).await?;

        assert_eq!(result.title, "ParamsNotebook");

        Ok(())
    }
}
