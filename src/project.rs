use llvm_ir::{Function, Module, Type};
use llvm_ir::module::{GlobalAlias, GlobalVariable};
use std::fs::DirEntry;
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// A `Project` is a collection of LLVM code to be explored,
/// consisting of one or more LLVM modules
pub struct Project {
    modules: Vec<Module>,
}

impl Project {
    /// Construct a new `Project` from a path to an LLVM bitcode file
    pub fn from_bc_path(path: impl AsRef<Path>) -> Result<Self, String> {
        Ok(Self {
            modules: vec![Module::from_bc_path(path)?],
        })
    }

    /// Construct a new `Project` from multiple LLVM bitcode files
    pub fn from_bc_paths<P>(paths: impl IntoIterator<Item = P>) -> Result<Self, String> where P: AsRef<Path> {
        Ok(Self {
            modules: paths
                .into_iter()
                .map(|p| Module::from_bc_path(p.as_ref()))
                .collect::<Result<Vec<_>,_>>()?,
        })
    }

    /// Construct a new `Project` from a path to a directory containing
    /// LLVM bitcode files.
    ///
    /// All files in the directory which have the extension `extn` will
    /// be parsed and added to the `Project`.
    pub fn from_bc_dir(path: impl AsRef<Path>, extn: &str) -> Result<Self, io::Error> {
        Ok(Self {
            modules: Self::modules_from_bc_dir(path, extn, |_| false)?,
        })
    }

    /// Construct a new `Project` from a path to a directory containing LLVM
    /// bitcode files.
    ///
    /// All files in the directory which have the extension `extn`, except those
    /// for which the provided `exclude` closure returns `true`, will be parsed
    /// and added to the `Project`.
    pub fn from_bc_dir_with_blacklist(path: impl AsRef<Path>, extn: &str, exclude: impl Fn(&Path) -> bool) -> Result<Self, io::Error> {
        Ok(Self {
            modules: Self::modules_from_bc_dir(path, extn, exclude)?,
        })
    }

    /// Add the code in the given LLVM bitcode file to the `Project`
    pub fn add_bc_path(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let module = Module::from_bc_path(path)?;
        self.modules.push(module);
        Ok(())
    }

    /// Add the code in the given directory to the `Project`.
    /// See [`Project::from_bc_dir()`](struct.Project.html#method.from_bc_dir).
    pub fn add_bc_dir(&mut self, path: impl AsRef<Path>, extn: &str) -> Result<(), io::Error> {
        let modules = Self::modules_from_bc_dir(path, extn, |_| false)?;
        self.modules.extend(modules);
        Ok(())
    }

    /// Add the code in the given directory, except for blacklisted files, to the `Project`.
    /// See [`Project::from_bc_dir_with_blacklist()`](struct.Project.html#method.from_bc_dir_with_blacklist).
    pub fn add_bc_dir_with_blacklist(&mut self, path: impl AsRef<Path>, extn: &str, exclude: impl Fn(&Path) -> bool) -> Result<(), io::Error> {
        let modules = Self::modules_from_bc_dir(path, extn, exclude)?;
        self.modules.extend(modules);
        Ok(())
    }

    /// Iterate over all `Function`s in the `Project`.
    /// Gives pairs which also indicate the `Module` the `Function` is defined in.
    pub fn all_functions(&self) -> impl Iterator<Item = (&Function, &Module)> {
        self.modules.iter().map(|m| m.functions.iter().zip(std::iter::repeat(m))).flatten()
    }

    /// Iterate over all `GlobalVariable`s in the `Project`.
    /// Gives pairs which also indicate the `Module` the `GlobalVariable` comes from.
    pub fn all_global_vars(&self) -> impl Iterator<Item = (&GlobalVariable, &Module)> {
        self.modules.iter().map(|m| m.global_vars.iter().zip(std::iter::repeat(m))).flatten()
    }

    /// Iterate over all `GlobalAlias`es in the `Project`.
    /// Gives pairs which also indicate the `Module` the `GlobalAlias` comes from.
    pub fn all_global_aliases(&self) -> impl Iterator<Item = (&GlobalAlias, &Module)> {
        self.modules.iter().map(|m| m.global_aliases.iter().zip(std::iter::repeat(m))).flatten()
    }

    /// Iterate over all named struct types in the `Project`.
    /// Gives triplets `(name, Type, Module)` which indicate the struct's name,
    /// type, and which module it comes from.
    ///
    /// If the `Type` in the triplet is `None`, that means the struct type is
    /// opaque; see
    /// [LLVM 8 docs on Opaque Structure Types](https://releases.llvm.org/8.0.0/docs/LangRef.html#t-opaque).
    pub fn all_named_struct_types(&self) -> impl Iterator<Item = (&String, Option<Type>, &Module)> {
        self.modules.iter()
            .map(|m| m.named_struct_types.iter()
                .map(|(name, opt)| (name, opt.as_ref().map(|arc| arc.read().unwrap().clone())))
                .zip(std::iter::repeat(m))
                .map(|((name, opt), m)| (name, opt, m))
            )
            .flatten()
    }

    /// Get the names of the LLVM modules which have been parsed and loaded into
    /// the `Project`
    pub fn active_module_names(&self) -> impl Iterator<Item = &String> {
        self.modules.iter().map(|m| &m.name)
    }

    /// Search the project for a function with the given name.
    /// If a matching function is found, return both it and the module it was
    /// found in.
    pub fn get_func_by_name<'p>(&'p self, name: &str) -> Option<(&'p Function, &'p Module)> {
        let mut retval = None;
        for module in &self.modules {
            if let Some(f) = module.get_func_by_name(name) {
                match retval {
                    None => retval = Some((f, module)),
                    Some((_, retmod)) => panic!("Multiple functions found with name {:?}: one in module {:?}, another in module {:?}", name, retmod.name, module.name),
                };
            }
        }
        retval
    }

    /// Search the project for a named struct type with the given name.
    /// If a matching named struct type is found, return both it and the module
    /// it was found in.
    ///
    /// If `None` is returned, then no named struct type with the given name was
    /// found in the project.
    ///
    /// If `Some(None, <module>)` is returned, that means the struct type is
    /// opaque; see
    /// [LLVM 8 docs on Opaque Structure Types](https://releases.llvm.org/8.0.0/docs/LangRef.html#t-opaque).
    ///
    /// If the named struct type is defined in multiple modules in the `Project`,
    /// this returns one of them arbitrarily. However, it will only return
    /// `Some(None, <module>)` if _all_ definitions are opaque; that is, it will
    /// attempt to return some non-opaque definition if one exists, before
    /// returning an opaque definition.
    pub fn get_named_struct_type_by_name<'p>(&'p self, name: &str) -> Option<(&'p Option<Arc<RwLock<Type>>>, &'p Module)> {
        let mut retval: Option<(&'p Option<Arc<RwLock<Type>>>, &'p Module)> = None;
        for module in &self.modules {
            if let Some(t) = module.named_struct_types.iter().find(|&(n, _)| n == name).map(|(_, t)| t) {
                match (retval, t) {
                    (None, t) => retval = Some((t, module)),  // first definition we've found: this is the new candidate to return
                    (Some(_), None) => {},  // this is an opaque definition, and we previously found some other definition (opaque or not); do nothing
                    (Some((None, _)), t@Some(_)) => retval = Some((t, module)),  // found an actual definition, replace the previous opaque definition
                    (Some((Some(arc1), retmod)), Some(arc2)) => {
                        // duplicate non-opaque definitions: ensure they completely agree
                        let def1: &Type = &arc1.read().unwrap();
                        let def2: &Type = &arc2.read().unwrap();
                        if def1 == def2 {
                            // they're true duplicates: do nothing
                        } else {
                            // this is a hack, not completely sure why it is necessary.
                            // Before immediately signalling the error, check if
                            // one of the definitions is an empty struct, and
                            // prefer the other
                            match (def1, def2) {
                                (Type::StructType { element_types, .. }, _) if element_types.is_empty() => {
                                    // prefer the new definition
                                    retval = Some((t, module));
                                },
                                (_, Type::StructType { element_types, .. }) if element_types.is_empty() => {
                                    // prefer the current definition, do nothing
                                },
                                _ => panic!("Multiple named struct types found with name {:?}: the first was from module {:?}, the other was from module {:?}.\n  First definition: {:?}\n  Second definition: {:?}\n", name, retmod.name, module.name, def1, def2),
                            }
                        }
                    },
                };
            }
        }
        retval
    }

    fn modules_from_bc_dir(path: impl AsRef<Path>, extn: &str, exclude: impl Fn(&Path) -> bool) -> Result<Vec<Module>, io::Error> {
        // warning, we use both `Iterator::map` and `Result::map` in here, and it's easy to get them confused
        path
            .as_ref()
            .read_dir()?
            .filter(|entry| match entry_is_dir(entry) {
                Some(true) => false,  // filter out if it is a directory
                Some(false) => true,  // leave in the ones that are non-directories
                None => true,  // also leave in errors, because we want to know about those
            })
            .map(|entry| entry.map(|entry| entry.path()))
            .filter(|path| match path {
                Ok(path) => match path.extension() {
                    Some(e) => e == extn && !exclude(path),
                    None => false,  // filter out if it has no extension
                },
                Err(_) => true,  // leave in errors, because we want to know about those
            })
            .map(|path| path.and_then(|path| Module::from_bc_path(path)
                .map_err(|s| io::Error::new(io::ErrorKind::Other, s))))
            .collect()
    }

    /// For testing only: construct a `Project` directly from a `Module`
    #[cfg(test)]
    pub(crate) fn from_module(module: Module) -> Self {
        Self {
            modules: vec![module],
        }
    }
}

/// Returns `Some(true)` if the entry is a directory, `Some(false)` if the entry
/// is not a directory, and `None` if there was an I/O error in trying to make
/// the determination, or if the original `entry` was an `Err`.
fn entry_is_dir(entry: &io::Result<DirEntry>) -> Option<bool> {
    match entry {
        Ok(entry) => entry.file_type().map(|ft| ft.is_dir()).ok(),
        Err(_) => None,
    }
    // one-liner for this function:
    // entry.as_ref().ok().and_then(|entry| entry.file_type().map(|ft| ft.is_dir()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file_project() {
        let proj = Project::from_bc_path(Path::new("tests/bcfiles/basic.bc"))
            .unwrap_or_else(|e| panic!("Failed to create project: {}", e));
        let (func, module) = proj.get_func_by_name("no_args_zero").expect("Failed to find function");
        assert_eq!(&func.name, "no_args_zero");
        assert_eq!(&module.name, "tests/bcfiles/basic.bc");
    }

    #[test]
    fn double_file_project() {
        let proj = Project::from_bc_paths(vec!["tests/bcfiles/basic.bc", "tests/bcfiles/loop.bc"].into_iter().map(Path::new))
            .unwrap_or_else(|e| panic!("Failed to create project: {}", e));
        let (func, module) = proj.get_func_by_name("no_args_zero").expect("Failed to find function");
        assert_eq!(&func.name, "no_args_zero");
        assert_eq!(&module.name, "tests/bcfiles/basic.bc");
        let (func, module) = proj.get_func_by_name("while_loop").expect("Failed to find function");
        assert_eq!(&func.name, "while_loop");
        assert_eq!(&module.name, "tests/bcfiles/loop.bc");
    }

    #[test]
    fn whole_directory_project() {
        let proj = Project::from_bc_dir("tests/bcfiles", "bc").unwrap_or_else(|e| panic!("Failed to create project: {}", e));
        let (func, module) = proj.get_func_by_name("no_args_zero").expect("Failed to find function");
        assert_eq!(&func.name, "no_args_zero");
        assert_eq!(&module.name, "tests/bcfiles/basic.bc");
        let (func, module) = proj.get_func_by_name("while_loop").expect("Failed to find function");
        assert_eq!(&func.name, "while_loop");
        assert_eq!(&module.name, "tests/bcfiles/loop.bc");
    }

    #[test]
    fn whole_directory_project_with_blacklist() {
        let proj = Project::from_bc_dir_with_blacklist(
            "tests/bcfiles",
            "bc",
            |path| path.file_stem().unwrap() == "basic",
        ).unwrap_or_else(|e| panic!("Failed to create project: {}", e));
        proj.get_func_by_name("while_loop").expect("Failed to find function while_loop, which should be present");
        assert!(proj.get_func_by_name("no_args_zero").is_none(), "Found function no_args_zero, which is from a file that should have been blacklisted out");
    }
}
