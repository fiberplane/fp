use clap::Clap;
use jsonnet::{JsonValue, JsonnetVm};

#[derive(Clap)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) {
    match args.subcmd {
        SubCommand::Expand(args) => handle_expand_command(args).await,
        SubCommand::Init(args) => handle_init_command(args).await,
    }
}

#[derive(Clap)]
pub enum SubCommand {
    #[clap(name = "init", about = "Initialize a empty template")]
    Init(InitArguments),

    #[clap(name = "expand", about = "Expand a template")]
    Expand(ExpandArguments),
}

#[derive(Clap)]
pub struct ExpandArguments {}

/// Create a new random (v4) based UUID encoded as base64 with UrlSafe
/// character set and no padding.
pub fn new_base64_uuid() -> String {
    let id = uuid::Uuid::new_v4();
    encode_uuid(&id)
}

/// Take a Uuid and then convert it to base64 with the UrlSafe character set and
/// no padding.
pub fn encode_uuid(id: &uuid::Uuid) -> String {
    encode_base64(id.as_bytes())
}

/// Encode input to a base64 string using the UrlSafe characters set and no
/// padding.
pub fn encode_base64<T: AsRef<[u8]>>(input: T) -> String {
    let config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
    base64::encode_config(input, config)
}

async fn handle_expand_command(arg: ExpandArguments) {
    // This should accept a template and optionally a model (json?) and then
    // write out the expanded content to stdout/specified path.

    let mut vm = JsonnetVm::new();
    vm.native_callback(
        "generateId",
        |vm, args| Ok(JsonValue::from_str(vm, &new_base64_uuid())),
        &[],
    );

    vm.import_callback(|vm, base, rel| {
        let contents = match rel.to_str() {
            Some("fiberplane.libsonnet") => include_str!("../files/fiberplane.libsonnet"),
            Some("cell.libsonnet") => include_str!("../files/cell.libsonnet"),
            Some("notebook.libsonnet") => include_str!("../files/notebook.libsonnet"),
            Some(_) => return Err("import not found".to_string()),
            None => return Err("import not found".to_string()),
        };

        Ok((base.into(), contents.to_string()))
    });

    let webhook_model = include_str!("../files/webhook.json");
    vm.tla_code("model", webhook_model);

    let input = include_str!("../files/sample.jsonnet");

    let output = vm.evaluate_snippet("sample.jsonnet", input).unwrap();

    println!("{}", output);

    // todo!();
}

#[derive(Clap)]
pub struct InitArguments {}

async fn handle_init_command(arg: InitArguments) {
    // This should take a hardcoded sample template and write it to
    // stdout/specified path.
    todo!();
}
