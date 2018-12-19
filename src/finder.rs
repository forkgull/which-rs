use crate::checker::CompositeChecker;
use crate::error::*;
use either::Either;
#[cfg(windows)]
use crate::helper::has_executable_extension;
use std::env;
use std::ffi::OsStr;
use std::iter;
use std::path::{Path, PathBuf};

pub trait Checker {
    fn is_valid(&self, path: &Path) -> bool;
}

trait PathExt {
    fn has_separator(&self) -> bool;

    fn to_absolute<P>(self, cwd: P) -> PathBuf
    where
        P: AsRef<Path>;
}

impl PathExt for PathBuf {
    fn has_separator(&self) -> bool {
        self.components().count() > 1
    }

    fn to_absolute<P>(self, cwd: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        if self.is_absolute() {
            self
        } else {
            let mut new_path = PathBuf::from(cwd.as_ref());
            new_path.push(self);
            new_path
        }
    }
}

pub struct Finder;

impl Finder {
    pub fn new() -> Finder {
        Finder
    }

    pub fn find<T, U, V>(
        &self,
        binary_name: T,
        paths: Option<U>,
        cwd: V,
        binary_checker: CompositeChecker,
    ) -> Result<impl Iterator<Item = PathBuf>>
    where
        T: AsRef<OsStr>,
        U: AsRef<OsStr>,
        V: AsRef<Path>,
    {
        let path = PathBuf::from(&binary_name);

        let binary_path_candidates = if path.has_separator() {
            // Search binary in cwd if the path have a path separator.
            Either::Left(Self::cwd_search_candidates(path, cwd).into_iter())
        } else {
            // Search binary in PATHs(defined in environment variable).
            let p = paths.ok_or(Error::CannotFindBinaryPath)?;
            let paths: Vec<_> = env::split_paths(&p).collect();

            Either::Right(Self::path_search_candidates(path, paths).into_iter())
        };

        Ok(binary_path_candidates.filter(move |p| binary_checker.is_valid(p)))
    }

    fn cwd_search_candidates<C>(binary_name: PathBuf, cwd: C) -> impl IntoIterator<Item = PathBuf>
    where
        C: AsRef<Path>,
    {
        let path = binary_name.to_absolute(cwd);

        Self::append_extension(iter::once(path))
    }

    fn path_search_candidates<P>(
        binary_name: PathBuf,
        paths: P,
    ) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        let new_paths = paths.into_iter().map(move |p| p.join(binary_name.clone()));

        Self::append_extension(new_paths)
    }

    #[cfg(unix)]
    fn append_extension<P>(paths: P) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        paths
    }

    #[cfg(windows)]
    fn append_extension<P>(paths: P) -> impl IntoIterator<Item = PathBuf>
    where
        P: IntoIterator<Item = PathBuf>,
    {
        // Sample %PATHEXT%: .COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC
        // PATH_EXTENSIONS is then [".COM", ".EXE", ".BAT", …].
        // (In one use of PATH_EXTENSIONS we skip the dot, but in the other we need it;
        // hence its retention.)
        lazy_static! {
            static ref PATH_EXTENSIONS: Vec<String> =
                env::var("PATHEXT")
                    .map(|pathext| {
                        pathext.split(';')
                            .filter_map(|s| {
                                if s.as_bytes()[0] == b'.' {
                                    Some(s.to_owned())
                                } else {
                                    // Invalid segment; just ignore it.
                                    None
                                }
                            })
                            .collect()
                    })
                    // PATHEXT not being set or not being a proper Unicode string is exceedingly
                    // improbable and would probably break Windows badly. Still, don't crash:
                    .unwrap_or(vec![]);
        }

        paths
            .into_iter()
            .flat_map(move |p| -> Box<dyn Iterator<Item = _>> {
                // Check if path already have executable extension
                if has_executable_extension(&p, &PATH_EXTENSIONS) {
                    Box::new(iter::once(p))
                } else {
                    // Appended paths with windows executable extensions.
                    // e.g. path `c:/windows/bin` will expend to:
                    // c:/windows/bin.COM
                    // c:/windows/bin.EXE
                    // c:/windows/bin.CMD
                    // ...
                    Box::new(PATH_EXTENSIONS.iter().map(move |e| {
                        // Append the extension.
                        let mut p = p.clone().into_os_string();
                        p.push(e);

                        PathBuf::from(p)
                    }))
                }
            })
    }
}
