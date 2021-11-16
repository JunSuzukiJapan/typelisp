use std::str::Chars;
use std::iter::Peekable;

use crate::{Object, Error};

pub struct Reader;

impl Reader {
    pub fn new() -> Reader {
        Reader {}
    }

    pub fn read(&self, s: &str) -> Result<Object, Error> {
        let mut stream = s.chars().peekable();
        self.read_from_peekable(&mut stream)
    }

    fn read_from_peekable(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        self.skip_whitespace(stream);

        if let Some(ch) = stream.peek() {
            match *ch {
                '(' => self.read_list(stream),
                ')' => {
                    return Err(Error::UnmatchedParen);
                },
                '\'' => self.read_quote(stream),
                // '\'' => {
                //     self.char_stream.next(); // skip '
                //     let obj = self.read();
                //     let quote = Object::new_symbol(String::from("quote"));
                //     let cons = Object::new_cons(Rc::new(obj), Rc::new(Object::Null));
                //     let cons = Object::new_cons(Rc::new(quote), Rc::new(cons));
                //     cons
                // },
                '\"' => self.read_string(stream),
                _ => {
                    if self.is_number(ch) {
                        self.read_number(stream)
                    }else{
                        self.read_symbol(stream)
                    }
                },
            }
        }else{
            Ok(Object::Null)
        }
    }

    fn read_quote(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        stream.next(); // skip '
        let obj = self.read_from_peekable(stream)?;
        let quote = Object::new_symbol(String::from("quote"));
        let cons = Object::new_cons(obj, Object::Null);
        let cons = Object::new_cons(quote, cons);
        Ok(cons)
    }

    fn read_list2(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        self.skip_whitespace(stream);

        if let Some(ch) = stream.peek() {
            if *ch == ')' {
                stream.next(); // skip ')'
                return Ok(Object::Null);
            }

            let obj = self.read_from_peekable(stream)?;

            // check cons pair
            self.skip_whitespace(stream);
            if let Some(c) = stream.peek() {
                if *c == '.' { // cons pair
                    stream.next(); // skip '.'
                    self.skip_whitespace(stream);
                    let right = self.read_from_peekable(stream)?;

                    self.skip_whitespace(stream);
                    if let Some(c) = stream.peek() {
                        if *c != ')' {
                            return Err(Error::UnmatchedParenWhileReadingConsPair);
                        }
                    }else{
                        panic!("illegal cons pair");
                    }

                    return Ok(Object::new_cons(obj, right));
                }
            }

            let left = self.read_list2(stream)?;
            Ok(Object::new_cons(obj, left))

        }else{
            Err(Error::IllegalEndWhileReadingList)
        }
    }

    fn read_list(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        stream.next(); // skip '('
        Ok(self.read_list2(stream)?)
    }

    fn read_string(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        stream.next(); // skip first '"'
        let mut buf = String::new();

        loop {
            let ch: char;

            if let Some(c) = stream.peek()  {
                if *c == '"' {
                    stream.next(); // skip '"'
                    break;
                }
                ch = *c;
            }else{
                return Err(Error::IllegalEndOfString);
            }

            if ch == '\\' {
                let c = stream.next();
                match c {
                    Some(c2) => buf.push(c2),
                    None => return Err(Error::IllegalEndOfEscapeSequence),
                }
            }else{
                stream.next();
                buf.push(ch);
            }
        }

        Ok(Object::new_string(buf))
    }

    fn read_symbol(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        let mut buf = String::new();

        loop {
            let ch: char;

            if let Some(c) = stream.peek() {
                if c.is_whitespace() || *c == '(' || *c ==')' {
                    break;
                }
                ch = *c;
            }else{
                break;
            }

            stream.next();
            buf.push(ch);
        }

        if buf == "true" {
            Ok(Object::True)
        }else if buf == "false" {
            Ok(Object::False)
        }else{
            Ok(Object::new_symbol(buf))
        }
    }

    fn read_number(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        let mut buf = String::new();

        loop {
            let ch: char;

            if let Some(c) = stream.peek() {
                ch = *c;
            }else{
                break
            }

            if !self.is_number(&ch) {
                break;
            }
            stream.next();
            buf.push(ch);
        }

        let val: i64 = buf.parse::<i64>().unwrap();
        Ok(Object::new_i64(val))
    }

    fn is_number(&self, ch: &char) -> bool {
        match *ch {
            '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => true,
            _ => false
        }
    }

    fn is_whitespace(&self, ch: &char) -> bool {
        *ch == ' ' || *ch == ' '
    }

    fn skip_whitespace(&self, peekable: &mut Peekable<Chars>){
        while let Some(ch) = peekable.peek() {
            if !self.is_whitespace(ch) {
                break;
            }
            peekable.next();
        }
    }
}