use napi::bindgen_prelude::*;
use napi_derive::napi;
use napi_modules::EnvExt;

#[napi(module_exports)]
pub fn module_exports(mut _exports: Object, env: Env) -> napi::Result<()> {
    if env.is_main()? {
        let process: Object = env.require("node:process")?;
        let args: Array = process.get_named_property("argv")?;
        let name: String = args
            .get(2)?
            .ok_or_else(|| napi::Error::from_reason("missing argument: name"))?;
        let version: String = process.get_named_property("version")?;
        println!("Hello {} from Rust! Node.js version: {}", name, version);
    }
    Ok(())
}
