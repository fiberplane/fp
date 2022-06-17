use termwiz::escape::{parser::Parser, Action};

/// A simple helper used to iterate over Actions returned by a termviz Parser.
/// This is simply to make life easier by not having to deal with the annoying
/// callback based `Parser::parse` method that makes it hard to deal with mutable
/// borrows. An alternative would be to use the `Parser::parse_vec` method but that
/// comes at the cost of a lot of allocations since we more or less get *one* `Action`
/// per byte of terminal output.
pub struct ParserIter<'a> {
    parser: &'a mut Parser,
    buffer: &'a [u8],
    position: usize,
}

impl<'a> ParserIter<'a> {
    pub fn new(parser: &'a mut Parser, buffer: &'a [u8]) -> Self {
        Self {
            parser,
            buffer,
            position: 0,
        }
    }
}

impl<'a> Iterator for ParserIter<'a> {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        match self.parser.parse_first(&self.buffer[self.position..]) {
            Some((action, read)) => {
                self.position += read;
                Some(action)
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_test() {
        assert_eq!(
            ParserIter::new(&mut Parser::new(), "abc".as_bytes()).collect::<Vec<_>>(),
            vec![Action::Print('a'), Action::Print('b'), Action::Print('c')]
        );
    }
}
