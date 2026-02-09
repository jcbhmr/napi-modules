use camino::Utf8PathBuf;
use derive_more::{From, Into};
use memoize::memoize;
use napi::bindgen_prelude::*;
use thiserror::Error;
use url::{ParseError, Url};
use std::{hash::{Hash, Hasher}, rc::Rc, result::Result};

pub(crate) trait Sealed {}

impl Sealed for Env {}

#[allow(private_bounds)]
pub trait EnvExt: Sealed {
    fn filename(&self) -> napi::Result<Utf8PathBuf>;
    fn require<T: FromNapiValue>(&self, id: impl AsRef<str>) -> napi::Result<T>;
    fn require_resolve(&self, id: impl AsRef<str>) -> napi::Result<String>;
    fn import(&self, specifier: impl AsRef<str>, options: Option<Object>) -> napi::Result<Promise<Object<'_>>>;
    fn import_meta_resolve(&self, specifier: impl AsRef<str>) -> napi::Result<String>;
    fn is_main(&self) -> napi::Result<bool>;
}

impl EnvExt for Env {
    fn filename(&self) -> napi::Result<Utf8PathBuf> {
        let file_url_string = self.get_module_file_name()?;
        let path = file_url_string_to_utf8_path_buf(&file_url_string).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(path)
    }
    fn require<T: FromNapiValue>(&self, id: impl AsRef<str>) -> napi::Result<T> {
        let require = require_for(self.clone().into()).map_err(|e| napi::Error::from_reason(e))?;
        let require = require.borrow_back(self)?;
        let module = require.call(id.as_ref())?;
        let module: T = unsafe { module.cast()? };
        Ok(module)
    }
    fn require_resolve(&self, id: impl AsRef<str>) -> napi::Result<String> {
        let require = require_for(self.clone().into()).map_err(|e| napi::Error::from_reason(e))?;
        let require = require.borrow_back(self)?;
        let require_resolve: Function<&str, String> = require.get_named_property("resolve")?;
        require_resolve.call(id.as_ref())
    }
    fn import(&self, specifier: impl AsRef<str>, options: Option<Object>) -> napi::Result<Promise<Object<'_>>> {
        let esm_helpers = esm_helpers_for(self.clone().into()).map_err(|e| napi::Error::from_reason(e))?;
        let esm_helpers = esm_helpers.get_value(self)?;
        let import: Function<FnArgs<(&str, Option<Object>)>, Promise<Object>> = esm_helpers.get_named_property("import")?;
        import.call((specifier.as_ref(), options).into())
    }
    fn import_meta_resolve(&self, specifier: impl AsRef<str>) -> napi::Result<String> {
        let esm_helpers = esm_helpers_for(self.clone().into()).map_err(|e| napi::Error::from_reason(e))?;
        let esm_helpers = esm_helpers.get_value(self)?;
        let import_meta_resolve: Function<&str, String> = esm_helpers.get_named_property("importMetaResolve")?;
        import_meta_resolve.call(specifier.as_ref())
    }
    fn is_main(&self) -> napi::Result<bool> {
        let require = require_for(self.clone().into()).map_err(|e| napi::Error::from_reason(e))?;
        let require = require.borrow_back(self)?;
        let main: Option<Object> = require.get_named_property("main")?;
        if let Some(main) = main {
            let self_path = self.filename()?;
            let main_path: String = main.get_named_property("filename")?;
            let main_path = Utf8PathBuf::from(main_path);
            Ok(self_path == main_path)
        } else {
            Ok(false)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
enum EsmHelpersPathForError {
    #[error("I/O error: {0}")]
    IoError(String),
}

#[memoize]
fn esm_helpers_path_for(addon_path: Utf8PathBuf) -> Result<Utf8PathBuf, EsmHelpersPathForError> {
    const ESM_HELPERS_JS: &str = r#"
        const _import = (specifier, options) => import(specifier, options);
        export { _import as "import" };
        export const importMetaResolve = (specifier) => import.meta.resolve(specifier);
    "#;
    let esm_helpers_path = addon_path.with_added_extension("esm-helpers.js");
    fs_err::write(&esm_helpers_path, ESM_HELPERS_JS).map_err(|e| EsmHelpersPathForError::IoError(e.to_string()))?;
    Ok(esm_helpers_path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
enum FileUrlStringToUtf8PathBufError {
    #[error("parse error: {0}")]
    UrlParseError(#[from] ParseError),
    #[error("scheme is not 'file'")]
    SchemeNotFile,
    #[error("to_file_path failed")]
    ToFilePathFailed,
    #[error("path is not valid UTF-8")]
    PathNotUtf8,
}

fn file_url_string_to_utf8_path_buf(file_url_string: &str) -> Result<Utf8PathBuf, FileUrlStringToUtf8PathBufError> {
    let url = Url::parse(file_url_string).map_err(FileUrlStringToUtf8PathBufError::UrlParseError)?;
    if url.scheme() != "file" {
        return Err(FileUrlStringToUtf8PathBufError::SchemeNotFile);
    }
    let path = url.to_file_path().map_err(|_| FileUrlStringToUtf8PathBufError::ToFilePathFailed)?;
    let utf8_path = Utf8PathBuf::from_path_buf(path).map_err(|_| FileUrlStringToUtf8PathBufError::PathNotUtf8)?;
    Ok(utf8_path)
}

#[derive(Clone, Copy, From, Into)]
#[repr(transparent)]
pub(crate) struct EnvEqHash(pub Env);

impl Eq for EnvEqHash {}

impl PartialEq for EnvEqHash {
    fn eq(&self, other: &Self) -> bool {
        let self_file_url_string = self.0.get_module_file_name().expect("get_module_file_name should succeed");
        let other_file_url_string = other.0.get_module_file_name().expect("get_module_file_name should succeed");
        self_file_url_string.eq(&other_file_url_string)
    }
}

impl Hash for EnvEqHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let file_url_string = self.0.get_module_file_name().expect("get_module_file_name should succeed");
        file_url_string.hash(state);
    }
}

#[memoize]
fn require_for(env: EnvEqHash) -> Result<Rc<FunctionRef<&'static str, Unknown<'static>>>, String> {
    let env = env.0;
    let global = env.get_global().map_err(|e| e.to_string())?;
    let process: Object = global.get_named_property("process").map_err(|e| e.to_string())?;
    let get_builtin_module: Function<&str, Unknown> = process.get_named_property("getBuiltinModule").map_err(|e| e.to_string())?;
    let module = get_builtin_module.call("node:module").map_err(|e| e.to_string())?;
    // SAFETY: `node:module` is an object.
    let module: Object = unsafe { module.cast().map_err(|e| e.to_string())? };
    let create_require: Function<&str, Function<&str, Unknown>> = module.get_named_property("createRequire").map_err(|e| e.to_string())?;
    let path = env.filename().map_err(|e| e.to_string())?;
    let require = create_require.call(path.as_str()).map_err(|e| e.to_string())?;
    let require = require.create_ref().map_err(|e| e.to_string())?;
    Ok(require.into())
}

#[memoize]
fn esm_helpers_for(env: EnvEqHash) -> Result<Rc<ObjectRef>, String> {
    let env = env.0;
    let path = env.filename().map_err(|e| e.to_string())?;
    let esm_helpers_path = esm_helpers_path_for(path).map_err(|e| e.to_string())?;
    let require = require_for(env.into())?;
    let require = require.borrow_back(&env).map_err(|e| e.to_string())?;
    let esm_helpers = require.call(esm_helpers_path.as_str()).map_err(|e| e.to_string())?;
    // SAFETY: `esm-helpers.js` is an object.
    let esm_helpers: Object = unsafe { esm_helpers.cast().map_err(|e| e.to_string())? };
    let esm_helpers = esm_helpers.create_ref().map_err(|e| e.to_string())?;
    Ok(esm_helpers.into())
}
