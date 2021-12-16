use std::ops;
use once_cell::sync::Lazy;
use num_bigint::{BigInt, ToBigInt};

#[derive(Clone, PartialEq, Debug)]
pub enum Number {
    I64(i64),
    Bignum(BigInt),
}

impl Number {
    pub fn from_i64(value: i64) -> Number {
        Number::I64(value)
    }
}

pub static NUMBER_ZERO: Lazy<Number> = Lazy::new(|| {
    Number::from_i64(0)
});

impl ops::Add<&Number> for Number {
    type Output = Number;

    fn add(self, rhs: &Number) -> Number {
        match (self, rhs) {
            (Number::I64(val1), Number::I64(val2))  => {
                if let Some(result) = val1.checked_add(*val2) {
                    Number::I64(result)
                }else{
                    let big1 = ToBigInt::to_bigint(&val1).unwrap();
                    let big2 = ToBigInt::to_bigint(val2).unwrap();
                    Number::Bignum(big1 + big2)
                }
            },
            (Number::I64(val1), Number::Bignum(big2)) => {
                let big1 = ToBigInt::to_bigint(&val1).unwrap();
                Number::Bignum(big1 + big2)
            },
            (Number::Bignum(big1), Number::I64(val2)) => {
                let big2 = ToBigInt::to_bigint(val2).unwrap();
                Number::Bignum(big1 + big2)
            },
            (Number::Bignum(big1), Number::Bignum(big2)) => {
                Number::Bignum(big1 + big2)
            },
        }
    }
}