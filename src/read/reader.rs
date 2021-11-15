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
        self.skip_whitespace(&mut stream);

        if let Some(ch) = stream.peek() {
            match *ch {
                // '(' => self.read_list(),
                // ')' => {
                //     panic!("unmatched ')'");
                // },
                // '\"' => self.read_string(),
                // '\'' => {
                //     self.char_stream.next(); // skip '
                //     let obj = self.read();
                //     let quote = Object::new_symbol(String::from("quote"));
                //     let cons = Object::new_cons(Rc::new(obj), Rc::new(Object::Null));
                //     let cons = Object::new_cons(Rc::new(quote), Rc::new(cons));
                //     cons
                // },
                _ => {
                    if self.is_number(ch) {
                        self.read_number(&mut stream)
                    }else{
                        self.read_symbol(&mut stream)
                    }
                },
            }
        }else{
            Ok(Object::Null)
        }
    }

    fn read_symbol(&self, stream: &mut Peekable<Chars>) -> Result<Object, Error> {
        let mut buf = String::new();

        loop {
            let ch: Option<char>;

            if let Some(c) = stream.peek() {
                if c.is_whitespace() || *c == '(' || *c ==')' {
                    break;
                }
                ch = Some(*c);
            }else{
                break;
            }

            stream.next();
            buf.push(ch.unwrap());
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
            let ch: Option<char>;

            if let Some(c) = stream.peek() {
                ch = Some(*c);
            }else{
                break
            }

            if !self.is_number(&ch.unwrap()) {
                break;
            }
            stream.next();
            buf.push(ch.unwrap());
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