use std::io;
use std::io::ErrorKind;

pub trait PathIterExt {
    fn path_components(&self) -> io::Result<PathIter<'_, impl Iterator<Item = &str>>>;
}

impl PathIterExt for &str {
    fn path_components(&self) -> io::Result<PathIter<'_, impl Iterator<Item = &str>>> {
        if !self.starts_with('/') {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Path must be absolute, i.e. start with a slash '/'",
            ));
        }
        let mut inner = self.split('/');
        Ok(PathIter {
            next: inner.find(|c| !c.is_empty()),
            inner,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathComponent<'a> {
    pub name: &'a str,
    pub is_last: bool,
}

#[derive(Debug, Clone)]
pub struct PathIter<'a, I: Iterator<Item = &'a str>> {
    inner: I,
    next: Option<&'a str>,
}

impl<'a, I: Iterator<Item = &'a str>> Iterator for PathIter<'a, I> {
    type Item = PathComponent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next;
        self.next = self.inner.find(|c| !c.is_empty());
        current.map(|name| PathComponent {
            name,
            is_last: self.next.is_none(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_components_requires_leading_slash() {
        assert!("".path_components().is_err());
        assert!("foo".path_components().is_err());
        assert!("foo/bar".path_components().is_err());
    }

    #[test]
    fn test_path_components_marks_last_component() {
        let components: Vec<_> = "/foo/bar/baz".path_components().unwrap().collect();
        assert_eq!(
            components,
            vec![
                PathComponent {
                    name: "foo",
                    is_last: false
                },
                PathComponent {
                    name: "bar",
                    is_last: false
                },
                PathComponent {
                    name: "baz",
                    is_last: true
                },
            ]
        );
    }

    #[test]
    fn test_ignores_empty_components() {
        let components: Vec<_> = "/foo///bar/".path_components().unwrap().collect();
        assert_eq!(
            components,
            vec![
                PathComponent {
                    name: "foo",
                    is_last: false
                },
                PathComponent {
                    name: "bar",
                    is_last: true
                },
            ]
        );
    }
}
