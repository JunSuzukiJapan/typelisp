<!-- translated-from: docs/ja/reference/functions/collections.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# 문자열, 문자, 컬렉션

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>`, `Deque<T>`.

## 1. 문자열 `string`

문자열은 변경할 수 없다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | 대문자로 바꾼다(ASCII만). CL의 `string-upcase`처럼 새 문자열을 반환한다. 문자열은 변경할 수 없으므로 파괴적인 `nstring-upcase`는 없고 이것이 그 역할을 한다 |
| `downcase` | `(downcase s)` | `string→string` | 소문자로 바꾼다(ASCII만). `nstring-downcase`의 역할도 한다 |
| `capitalize` | `(capitalize s)` | `string→string` | 각 단어의 첫 글자를 대문자로, 나머지를 소문자로 한다(CL의 `string-capitalize`). 단어는 영숫자가 최대한 이어진 것 |
| `length` | `(length s)` | `string→int` | 문자 수 |
| `ref` | `(ref s i)` | `(string,int)→char` | `i`번째 문자. 범위를 벗어나면 panic |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | 부분 문자열 `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | 연결. 세 개 이상도 줄 수 있다(`(concatenate 'string ...)`와 같다) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | 사전순 비교 |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | 엄격한 사전순 미만(`<`와 같다) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | 동일성 비교(내용이 같은지가 아니라 같은 객체인지) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | 내용 비교(대소문자 구별) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | 내용 비교(대소문자 무시, ASCII만) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | 내용이 다른지(CL의 `string/=`. 가변 인수 형식은 이웃한 쌍을 비교한다) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | 대소문자를 무시한 순서(CL의 `string-lessp` 등). 공통 접두사가 있으면 짧은 쪽이 작다 |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | `c`를 `n`개 늘어놓은 문자열(CL의 `make-string`) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | `sub`가 처음 나타나는 위치. **CL의 `search`는 인수 순서가 반대**(`(search pattern sequence)`). 빈 문자열은 0에서 발견된다. 키워드는 [시퀀스의 키워드 인수](sequences.md#6-키워드-인수) 참고 |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | 처음으로 다른 위치. `equal`일 때만 `none`. 한쪽이 다른 쪽의 접두사이면 짧은 쪽의 끝. 키워드는 위와 같다 |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | `bag`에 포함된 문자를 양 끝 / 왼쪽 / 오른쪽에서 없앤다(CL의 `string-trim` 등). `bag`이 없으면 공백 `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | `sep`로 나눈다. CL에는 대응하는 것이 없다. 구분자가 이어지면 빈 요소가 생긴다. `sep`가 비어 있으면 panic |
| `to-string` | `(to-string x)` | `T→string` | `~a`처럼 문자열로 바꾼다. `int`/`i32`/`f64`/`bool`/`char`/`string`에 구현되어 있다(CL의 `princ-to-string`) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | UTF-8로 부호화한다(각 요소는 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | 복호화한다. 올바른 UTF-8이 아니면 `none` |

## 2. 문자 `char`

`char`는 Unicode 스칼라 값이다. 대소문자 변환과 분류는 ASCII 범위만 다룬다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | 대문자로 바꾼다(ASCII만) |
| `downcase` | `(downcase c)` | `char→char` | 소문자로 바꾼다(ASCII만) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | 코드 포인트로 비교 |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | 코드 포인트로 엄격한 미만(`<`와 같다) |
| `alphap` | `(alphap c)` | `char→bool` | ASCII 영문자인지 |
| `digitp` | `(digitp c)` | `char→bool` | ASCII 숫자인지 |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | 값을 비교한다 |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | 대소문자를 무시하고 값을 비교한다(CL의 `char-equal`) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | 값이 다른지(CL의 `char/=`. **가변 인수 형식은 이웃한 쌍을 비교한다**. 모든 쌍이 다른지 묻는 CL과 다르다) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | 대소문자를 무시한 순서(CL의 `char-lessp` 등) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | 대문자 / 소문자 / 애초에 대소문자 구별이 있는지(CL의 `upper-case-p` 등) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | 영문자 또는 숫자(CL과 같은 이름) |
| `graphicp` | `(graphicp c)` | `char→bool` | 인쇄할 수 있는지. 공백은 포함하고 줄바꿈과 탭은 포함하지 않는다(CL의 `graphic-char-p`) |
| `standardp` | `(standardp c)` | `char→bool` | CL의 표준 문자 96개 중 하나인지. 즉 `graphicp`와 줄바꿈(CL의 `standard-char-p`) |
| `char->int` | `(char->int c)` | `char→int` | Unicode 스칼라 값(반대는 [수](numbers.md#1-고정-폭-정수)의 `int->char`/`try-int->char`). CL의 `char-code`/`char-int`에 해당한다 |
| `char->string` | `(char->string c)` | `char→string` | 한 글자 문자열. CL의 `string` 함수는 지정자를 받아 이것을 맡지만, 이 언어에는 지정자가 없으므로 방향을 이름에 넣었다 |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | 그 기수에서 숫자의 **무게**(CL의 `digit-char-p`). `digitp`는 `bool`을 반환하는 다른 함수이다 |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | 무게 `w`에 해당하는 문자. 10 이상은 대문자(CL의 `digit-char`. 기수는 36까지) |
| `char->name` | `(char->name c)` | `char→Option<string>` | 문자의 이름. 이름이 있는 것은 리더가 읽을 수 있는 이름 붙은 문자뿐이다(CL의 `char-name`) |
| `name->char` | `(name->char s)` | `string→Option<char>` | 이름에 해당하는 문자. 대소문자를 구별하지 않고 리더의 별칭(`linefeed`/`null`)도 받는다(CL의 `name-char`) |

`char-code-limit`에 해당하는 상수는 없다(`char`의 상한은 언어가 아니라 Unicode가 정한다).

## 3. `Vector<T>`

늘어나는 배열.
값은 `#(1 2 3)`으로 쓸 수 있다([문법 레퍼런스](../syntax.md#1-어휘-요소). 요소의 타입은 문맥이나 첫 요소에서 정해지고, 평가할 때마다 새 벡터가
만들어진다). 출력도 `#(1 2 3)`이다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | 빈 벡터를 만든다. 타입 인수는 기대 타입에서 정해지므로 그냥 `let`에서는 `(the Vector<i32> (Vector::new))`라고 쓴다 |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x`를 `n`개 |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | 끝에 추가한다 |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | `i`번째 요소를 읽는다. 범위를 벗어나면 panic |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | `i`번째 요소를 바꾼다. 범위를 벗어나면 panic. `(setf (get v i) x)`로도 쓸 수 있다 |
| `len` | `(len v)` | `Vector<T>→int` | 요소 수 |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | 마지막 요소를 없애고 반환한다. 비어 있으면 `None`(`get`/`set`과 달리 panic하지 않는다) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | `Iter`를 구현한 이터레이터를 만든다 |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | 같은 요소가 없으면 `x`를 추가한다(CL의 `pushnew`. 장소를 고쳐 쓸 필요가 없으므로 매크로가 아니라 메서드) |

`map`/`filter` 등은 [시퀀스 함수](sequences.md#4-iter에-대한-시퀀스-함수)이며, `(map (iter v) f)`처럼 벡터를 `iter`에
통과시켜 넘긴다. 파괴적 연산(`nreverse`, `delete` 등)은 [파괴적 연산](sequences.md#7-파괴적-연산)에 있다.

## 4. `HashTable<K,V>`

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | 빈 테이블을 만든다 |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | 조회 |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | 삽입 또는 덮어쓰기 |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | 항목을 없애고, 있었다면 이전 값을 반환한다 |
| `count` | `(count h)` | `HashTable<K,V>→int` | 항목 수 |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | 모두 없앤다 |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | 키의 스냅숏 |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | 값의 스냅숏 |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | `(k . v)` 쌍의 스냅숏 |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | `Iter`를 구현한 이터레이터. 요소는 `(k . v)`의 `cons-cell`. CL의 `with-hash-table-iterator`에 해당하며 `doiter`/`map`/`filter` 등이 그대로 동작한다 |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | CL의 `maphash` |
| `size` | `(size h)` | `HashTable<K,V>→int` | CL의 `hash-table-size`. 이 테이블에서는 차 있는 항목 수(`count`와 같다) |

**`Hash`를 구현한 타입이라면 무엇이든 키가 될 수 있다.** `defstruct`/`defenum`의 타입도 포함한다. `get`/`set`/`remove`는
`(where (Hash K))`를 가지므로, 그것을 구현하지 않은 타입을 키로 하는 테이블은 **타입 오류**이다(`f64`는 `NaN` 때문에
`Hash`가 없다).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; fixnum에 들어가는 음이 아닌 값을 반환한다
```

구현되어 있는 타입: `int`와 고정 폭 정수 6가지, `bool`, `char`, `string`, `symbol`(부동소수점 수에는 없다). 직접 만든
타입에서는 결과를 `*sxhash-mask*`(2^30-1)와 `logand`해서 음이 아니게 한다. 문자열을 해시하려면 `string`의 구현이 쓰는
`(sxhash-string s)`(32비트 FNV-1a)를 호출할 수 있다.

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

두 키가 같은지는 객체의 동일성이 아니라 **키 타입 자신**(`sxhash`와, `Hash`의 상위 트레이트 `Eq`의 `equals`)이 정한다.
그래서 위처럼 "다른 값이지만 같은" 키로 조회할 수 있다.

`sxhash`는 충돌해도 된다(`Hash`의 약속은 한 방향뿐이다. 같은 값은 같은 해시를 가져야 한다). 충돌한 키는 `equals`로
구별된다.

## 5. `Array<T>`(다차원 배열)

표준 라이브러리의 `defstruct`이다. 내장 타입이 아니므로 `defstruct`로 할 수 있는 일은 모두 할 수 있다.
값은 `#2A((1 2) (3 4))`로 쓸 수 있다([문법 레퍼런스](../syntax.md#1-어휘-요소)).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | CL의 `make-array`. `dims`는 복사된다. `init`은 모든 칸의 초깃값(CL의 `:initial-element`. 이 언어에는 "묶이지 않은 칸"이 없으므로 필수). `:fill-pointer`는 1차원에서만 |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | CL의 `aref` / `(setf (aref …))`. 인덱스가 범위를 벗어나면 panic |
| `aref` | `(aref a i j …)` | — | 인덱스를 그대로 늘어놓는 CL의 표기. 위의 `get`/`set`으로 전개된다. `(setf (aref a i j) v)`도 된다 |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | CL의 `row-major-aref`. 평평한 인덱스 |
| `rank` | `(rank a)` | `Array<T>→int` | CL의 `array-rank` |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | CL의 `array-dimension` |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | CL의 `array-dimensions`. CL이 새 리스트를 반환하는 것처럼 **사본**을 반환한다 |
| `total-size` | `(total-size a)` | `Array<T>→int` | CL의 `array-total-size`(할당된 칸 수. 채움 포인터와는 관계없다) |
| `len` | `(len a)` | `Array<T>→int` | 배열에 대한 CL의 `length`. 채움 포인터가 있으면 그것, 없으면 `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | CL의 `array-in-bounds-p`. 인덱스의 **개수**가 틀려도 오류가 아니라 거짓 |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | CL의 `array-row-major-index` |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | CL의 `adjust-array`. 랭크는 바꿀 수 없다. 범위 안에 남는 요소는 같은 인덱스에 남고 새 칸은 `init`이 된다. CL과 달리 배열을 반환하지 않는다(이 언어의 배열은 모두 크기를 바꿀 수 있으므로 반환할 두 번째 배열이 없다) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | CL의 `vector-push-extend`. 채움 포인터가 없으면 panic |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | CL의 `vector-pop`. 비어 있으면 `none` |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | 채움 포인터(없으면 `none`). `(setf a::fill-pointer …)`로 쓸 수 있다 |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | 행 우선 순서의 이터레이터. 채움 포인터가 있으면 거기서 멈춘다 |

- **인덱스는 `Vector<int>`이다.** 메서드는 "같은 타입의 인수를 끝에 몇 개든"이라고 선언할 수 없으며, `aref` 표기가 그
  틈을 메운다.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`는 **없다**. 받는 쪽의
  정적 타입이 이미 그 질문에 답한다.
- `Array::new`는 `defstruct`가 만드는 필드 순서의 생성자이며 배열을 만드는 데 쓰는 것이 아니다. `Array::make`를 쓴다.
- **배열은 CL의 배열 구문으로 출력된다.** 랭크 1은 `#(1 2 3)`, 그 밖의 랭크는 `#nA` 뒤에 그 수만큼 괄호를 겹친 것
  (`#2A((1 2 3) (4 5 6))`), 랭크 0은 `#0A5`. 채움 포인터가 있으면 거기서 출력을 멈춘다.
  `*print-array*`([출력](printing.md#6-출력량-제어))를 거짓으로 하면 모양만 `#<array 2x3>`으로 출력한다. 요소가
  `print-object`가 없는 `defstruct`인 배열만 내장 형식 `#<array<...> ...>`으로 출력된다(오류는 아니다).

## 6. `BitVector`(비트 벡터)

고정 길이의 비트 열. 표준 라이브러리의 `defstruct`이다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | 길이 `n`, 모든 비트가 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | 범위를 벗어나면 panic |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | CL의 표기. `(setf (bit v i) b)`도 된다. CL의 `sbit`는 단순 비트 벡터를 요구하는 점만 `bit`와 다르지만, 이 언어에는 비트 벡터가 한 종류뿐이다 |
| `len` | `(len v)` | `BitVector→int` | 비트 수 |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | 새 비트 벡터를 반환한다. 길이가 다르면 panic. CL에 있는 세 번째 인수(결과를 넣을 곳)는 없다 |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | 보수 |

`bit-vector-p`는 없다(정적 타입이 답한다).

## 7. `HashSet<T>`

중복 없는 요소의 모임(Rust의 `HashSet`). 표준 라이브러리의 `defstruct`이며 내용은 `HashTable<T,()>`이다. 요소 타입은 `HashTable`의
키와 마찬가지로 `Hash`를 구현해야 한다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `make` | `(HashSet::make)` | `()→HashSet<T>` | 빈 집합을 만든다. 타입 인수는 기대 타입에서 정해진다 |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | `x`를 추가한다. 새로 들어가면 `true`, 이미 있으면 `false` |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | `x`가 있는지 |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | `x`를 뺀다. 있었으면 `true` |
| `count` | `(count s)` | `HashSet<T>→int` | 요소 수 |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | 모두 지운다 |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | 요소의 이터레이터. 순서는 정해져 있지 않다 |

```lisp
(let ((seen (the HashSet<string> (HashSet::make))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

7~9장의 세 타입에 공통되는 점:

- 만들 때는 `make`를 쓴다. `new`는 `defstruct`가 생성하는 필드 순서의 생성자이며 만들 때 쓰는 것이 아니다(`Array::make`와 같다).
- `iter`는 호출한 시점의 사본을 순회한다. `doiter` 안에서 같은 컬렉션을 바꿔도 그 루프에는 보이지 않는다.
- 요소 타입이 `print-object`를 구현하면 `#<hashset "a" "b">` `#<sortedtable 1 "a">` `#<deque 1 2>` 형태로 요소를
  출력한다.

## 8. `SortedTable<K,V>`

키의 오름차순으로 정렬된 표(Rust의 `BTreeMap`). 키 타입은 `Ord`를 구현해야 한다. 키와 값을 키 순서로 늘어놓은 두 개의 `Vector`로 가지며, 검색은 이진
탐색이다. 새 키의 `set`과 `remove`는 그 위치보다 뒤의 요소를 민다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `make` | `(SortedTable::make)` | `()→SortedTable<K,V>` | 빈 표를 만든다 |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | 검색 |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | 삽입 또는 덮어쓰기 |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | 삭제하고, 있었으면 이전 값을 돌려준다 |
| `count` | `(count t)` | `SortedTable<K,V>→int` | 요소 수 |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | 모두 지운다 |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | 키를 작은 순서로 |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | 값을 키 순서로 |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | 키 순서의 `#{키 값}` 튜플 |

```lisp
(let ((t (the SortedTable<string,int> (SortedTable::make))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 와 pear 3
```

## 9. `Deque<T>`

양쪽 끝에서 넣고 뺄 수 있는 열(Rust의 `VecDeque`).

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `make` | `(Deque::make)` | `()→Deque<T>` | 빈 열을 만든다 |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | 앞 / 뒤에 추가한다 |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | 앞 / 뒤의 요소를 꺼내 돌려준다. 비어 있으면 `none` |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | 앞 / 뒤의 요소를 본다(꺼내지 않는다) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | 앞에서부터 `i`번째. 범위 밖이면 `none` |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | `i`번째를 덮어쓴다. 범위 밖이면 panic |
| `count` | `(count d)` | `Deque<T>→int` | 요소 수 |
| `clear` | `(clear d)` | `Deque<T>→Unit` | 모두 지운다 |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | 앞에서부터 차례로 |

```lisp
(let ((q (the Deque<int> (Deque::make))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```
