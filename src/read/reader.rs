use std::str::Chars;
use std::iter::Peekable;
// use std::rc::Rc;

use crate::{Object, Cons, Error};

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
        let cons = Cons::new_cons(obj, None);
        let cons = Cons::new_cons(quote, Some(cons));
        Ok(Object::List(cons))
    }

    fn read_cons(&self, stream: &mut Peekable<Chars>) -> Result<Option<Cons>, Error> {
        let result: Option<Cons> = None;

        self.skip_whitespace(stream);
        if let Some(ch) = stream.peek() {
            if *ch == ')' {
                stream.next(); // skip ')'
                return Ok(result);
            }
        }else{ // None
            return Err(Error::IllegalEndWhileReadingList);
        }

        let obj = self.read_from_peekable(stream)?;
        let cdr = self.read_cons(stream)?;
        Ok(Some(Cons::new_cons(obj, cdr)))
    }

    fn read_list(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        stream.next(); // skip '('
        let cons = self.read_cons(stream)?;
        if cons.is_none() {
            Ok(Object::Null)
        }else{
            let l = cons.unwrap();
            Ok(Object::List(l))
        }
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
        }else if buf == "null" {
            Ok(Object::Null)
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