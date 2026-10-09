<!-- translated-from: docs/ja/reference/functions/sequences.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# 쌍, S 식, 시퀀스

제네릭 쌍 `cons-cell`, S 식 데이터 `Sexpr`, 심볼, `Iter` 위에 쓰인 시퀀스 함수, 고차 함수.

## 1. 쌍 `cons-cell<A,B>`

`cons`/`car`/`cdr`는 **제네릭 쌍 타입 `cons-cell<A,B>`**(표준 라이브러리의 `defstruct`)의 생성자와 필드 접근자이다. 필드는
`변수::car`/`변수::cdr`([문법 레퍼런스](../syntax.md#36-defstruct--구조체사용자-정의-타입)의 `defstruct` 접근자 구문)로도
`(car 변수)`/`(cdr 변수)`로도 읽을 수 있다. 바꾸려면 `(setf 변수::car v)`/`(setf 변수::cdr v)`를 쓴다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | 쌍을 만든다 |
| `car` | `(car p)` | `cons-cell<A,B>→A` | 첫 번째 요소 |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | 나머지 |

`cons-cell`은 튜플 구문의 대신이기도 하다. 다중 값을 반환하는 CL 함수(`floor`의 몫과 나머지, `read-from-string`의 값과 위치
등)는 이 언어에서는 `cons-cell`을 반환한다.

## 2. S 식 데이터 `Sexpr`

`read`가 반환하는 데이터 타입 `Sexpr`에는 18개의 변형이 있다:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array`.
`vector`와 `array`는 `#(..)`와 `#nA(..)`로 쓴 데이터이며([문법 레퍼런스](../syntax.md#1-어휘-요소)), 각각
`Vector<Option<Sexpr>>`와 `Array<Option<Sexpr>>`를 담는다. `(vector v)`로 묶은 `v`에는 `len`이나 `get`을 그대로 쓸 수
있다.
S 식의 셀은 1장의 일반적인 `cons`/`car`/`cdr`가 아니라 `sexpr-*` 함수로 다룬다. 주로 `defmacro`의 본체에서 형식을 만들고
분해하는 데 쓴다.

**S 식 데이터의 타입은 `Option<Sexpr>`이다.** 빈 리스트는 `Sexpr`의 변형이 아니라 `Option`의 `none`이며, `Sexpr` 자체는
"비어 있지 않은 S 식"을 뜻한다. 그래서 `sexpr-*` 함수는 `Option<Sexpr>`를 받고 반환한다.

- `Option<Sexpr>`가 기대되는 곳의 `()`는 빈 리스트이다(`(Option::none)`으로도 쓸 수 있다)
- `Sexpr`는 `Option<Sexpr>`가 기대되는 곳에서 암묵적으로 넓어진다(실행 시 변환 없음). 반대 방향, 즉 `Option<Sexpr>`를
  `Sexpr`로 쓰는 것은 "이것은 빈 리스트가 아니다"라고 주장하는 것이므로 `match`나 `unwrap`으로 명시해야 한다
- `match`에서는 `Sexpr`의 18개 변형과 `none`을 **같은 갈래 목록에 평평하게** 쓸 수 있다
  ([문법 레퍼런스](../syntax.md#43-match--패턴-매칭))

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | `Sexpr` 셀을 만든다 |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | 첫 번째 요소. **빈 리스트에는 빈 리스트**(CL과 같다). `Cons`가 아닌 아톰이면 panic |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | 나머지. **빈 리스트에는 빈 리스트**(CL과 같다). `Cons`가 아닌 아톰이면 panic |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | `Cons`인지 |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | 빈 리스트인지 |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | `Cons`가 아닌지 |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | `Sym`(심볼)인지 |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | `int` 변형의 내용(fixnum 또는 다배정도). 다른 타입이면 panic |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | 그 폭의 변형의 내용. 다른 타입이면 panic |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | 부동소수점 변형의 내용. 다른 타입이면 panic |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | `Char`의 내용. 다른 타입이면 panic |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | `Bool`의 내용. 다른 타입이면 panic |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | `Str`의 내용. 다른 타입이면 panic |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | `Sym`의 이름. 다른 타입이면 panic |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 동일성 비교(`Cons`/`Str`는 객체의 동일성, 나머지는 값을 비교한다) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | 구조의 동등(`Cons`는 재귀적으로, `Str`는 내용으로) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | `equal`에 더해 대소문자를 무시한 비교와 타입을 넘는 수의 비교 |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | 두 `Sexpr` 리스트를 (비파괴적으로) 잇는다. `,@`는 이것으로 전개된다 |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | `Sexpr` 리스트의 각 요소에 `f`를 적용한 새 `Sexpr` 리스트(4장의 `map`은 `Iter`용이며 `Sexpr` 리스트를 순회할 수 없다) |

수 접근자가 타입마다 하나씩 9개 있는 것은 `Sexpr`가 "값의 타입이 다른 어디에도 쓰여 있지 않은 유일한 곳"이기 때문이다.
`Sexpr`에 넣은 `u8`은 `u8` 변형으로 들어가 `(sexpr-u8 s)`로만 나온다. `(sexpr-int s)`에 넘기면 panic하며, 답을 조용히
넓히는 일은 없다. 읽어 들인 데이터(`'(1 2 3)`, 매크로 인수)의 정수는 `int` 변형이며 `(sexpr-int s)`로 읽는다.

`Sexpr` 리스트에는 `rplaca`/`nconc` 같은 파괴적 연산이 없다. `Sexpr` 셀은 만든 뒤에 바꿀 수 없다.

## 3. 심볼

`symbol`은 심볼 자체의 타입이다. `Sexpr`가 요구되는 곳에서는 암묵적으로 변환되지만 반대 방향으로는 자동으로 변환되지
않는다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | 심볼의 이름을 꺼낸다 |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | 문자열에서 심볼을 만든다(인턴한다) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | 키워드(`:name`)인지. 콜론은 이름의 일부이므로 첫 글자를 보고 판단한다([문법 레퍼런스](../syntax.md#1-어휘-요소)) |

`gensym`은 [매크로](system.md#8-매크로)를 참고한다.

## 4. `Iter`에 대한 시퀀스 함수

시퀀스 함수는 **`Iter` 트레이트 위의 제네릭 함수**이다. 컬렉션에서 `(iter coll)`로 이터레이터를 얻어 넘긴다(`Vector<T>` /
`HashTable<K,V>` / `Array<T>`가 이를 지원한다. `Sexpr` 리스트는 `Iter`를 구현하지 않으므로 이 함수들의 대상이 아니다).
**결과 컬렉션은 새 `Vector`로 반환된다.** 표의 `Iter<A>`는 "`Item`이 `A`인 `Iter`의 어떤 구현"이라는 뜻이다. 반환된
`Vector`를 다시 순회하려면 `(iter result)`를 넘긴다.

술어를 받는 함수(CL의 `-if` 계열에 해당):

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | 사상 |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 술어를 만족하는 요소만 |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | 술어를 만족하는 요소를 없앤다 |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | 술어를 만족하는 첫 요소 |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | 술어를 만족하는 첫 위치 |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | 술어를 만족하는 개수 |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 모든 요소가 술어를 만족하는지 |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | 술어를 만족하는 요소가 있는지(CL의 `some`에 해당. `Some` 생성자와 겹치지 않는 이름) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | 왼쪽 접기 |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | 오른쪽 접기 |

인덱스, 길이, 잘라 내기:

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | 요소 수 |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | 이터레이터를 잇는다. 세 개 이상도 줄 수 있다 |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | CL의 `concatenate`. 결과 타입은 **인용한 심볼 리터럴**로 쓴다(CL은 실행 시의 타입 지정자). `'vector`는 하나 이상, `'string`은 0개 이상(0개면 `""`). `Sexpr` 리스트는 대상이 아니다(`sexpr-append`를 쓴다) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | 뒤집기(비파괴적) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | `n`번째 요소(범위를 벗어나면 `None`) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | 인수 순서가 반대인 `nth` |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | 앞의 `n`개 |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)`(`end`는 길이로 잘린다) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | 마지막 **요소**(CL처럼 "마지막 셀"이 아니다) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | 마지막 요소를 뺀 나머지 |

`Eq` / `Ord` 경계가 필요한 함수(술어 대신 트레이트로 비교한다. [표준 트레이트](traits.md#2-eq--ord비교)):

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | `x`와 같은 요소가 있는지(CL과 달리 리스트의 나머지가 아니라 `bool`) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | `x`와 같은 첫 요소 |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | `x`와 같은 첫 위치 |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | `x`와 같은 요소의 개수 |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | CL의 `(sort sequence predicate)`. 안정적이고 비파괴적인 정렬. `cmp`는 "첫 번째 인수가 두 번째보다 엄격하게 앞에 온다"일 때 `true` |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | `car`가 `k`와 같은 첫 쌍. 값은 `(cdr p)`로 꺼낸다 |

이것들과 5장의 함수 상당수는 CL의 키워드 인수 `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count`도
받는다(6장).

## 5. CL 시퀀스 함수의 나머지

모두 4장처럼 `Iter` 위의 제네릭 함수이다. 결과 컬렉션은 새 `Vector`로 반환된다.

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | CL의 이름 붙은 인덱스 |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | 첫 요소를 뺀 나머지(공유하는 꼬리가 아니라 새 `Vector`) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | 이터레이터를 `Vector`로 실체화한다(CL의 `copy-seq`/`copy-list`) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | 뒤집은 `a` 뒤에 `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x`를 `n`개(CL의 `make-list`/`make-sequence`). `Vector::new`처럼 타입 인수는 기대 타입에서 정해지므로 그냥 `let`에서는 `the`가 필요하다 |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `member`처럼 **`bool`**(이터레이터에는 반환할 꼬리가 없다) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | `any`/`every`의 부정 |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | 긍정판과 같은 타입 | 술어를 부정한 판 |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | 값으로 없앤다 |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | 중복을 없앤다. CL처럼 **마지막 것을 남긴다**(`:from-end true`면 첫 것을 남긴다) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | 값 / 술어로 바꾼다 |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | `Iter<cons-cell<K,V>>`에 대해 | `assoc`의 술어판과 값 쪽 판 |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | 앞에 쌍을 추가한다 |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | 두 시퀀스를 짝짓는다. 짧은 쪽에서 멈춘다 |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | 여러 시퀀스에 대한 CL의 `mapcar`. 짧은 쪽에서 멈춘다 |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | 부작용을 위한 사상 |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | 사상하고 잇는다 |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | 차례로 이어지는 **꼬리**에 대해 사상한다 |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | 부작용을 위해 꼬리에 대해 사상한다(`mapc`의 `maplist`판) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | 꼬리에 대해 사상하고 잇는다(`mapcan`의 `maplist`판) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | `sub`가 처음 나타나는 위치. 받는 쪽이 `string`이면 `string`의 메서드가 선택된다([문자열](collections.md#1-문자열-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | 처음으로 다른 위치. 같으면 `none` |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | 병합. CL은 정렬된 입력을 요구하지만 이것은 이은 것을 정렬한다 |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | `x`가 없으면 **앞에** 추가한다 |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | 집합 연산. CL은 순서를 정하지 않지만 여기서는 안정적이며 **처음 나타난 순서**이다 |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | 포함 |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | 접미사인지 / 접미사 앞의 부분. CL은 **공유 구조**에 대해 묻지만 공유할 구조가 없으므로 이것은 **값으로서의** 접미사를 묻는다 |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | 요소별 동등. `Vector<T>` 자체는 `Eq`를 구현하지 않는다 |
| `caar`…`cddddr` | `(cadr p)` | 중첩된 쌍에 대해 | CL의 28개 함수. **리스트가 아니라 쌍**을 따라간다. `cadr`는 `cons-cell<A,cons-cell<B,C>>`를 받는다 |

CL에 있고 이 언어에 없는 것: `list*`(꼬리를 바꿔 끼운 부적절한 리스트라는 개념이 없다),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if`(임의 깊이의 이질적인 트리를 순회하는 것을 기술할 수 있는 타입이 없다.
`Sexpr`의 트리라면 `equal`이 `tree-equal`에 해당한다), 속성 리스트 계열 `getf`/`get-properties`/`symbol-plist`/`remprop`(키와
값을 번갈아 늘어놓은 타입 없는 리스트라는 표현이 없다. `assoc`(연관 리스트)나 `HashTable`이 같은 역할을 한다), 그리고
`Vector<T>`와 `Sexpr` 리스트 사이의 변환 함수(`Sexpr` 리스트의 요소는 각각 다른 타입일 수 있으므로 하나의 요소 타입 `T`로
쓸 수 없다).

## 6. 키워드 인수

4장과 5장의 함수는 CL의 시퀀스 키워드 `:key` / `:test` / `:test-not` / `:start` / `:end` / `:from-end` / `:count`를 받는다.
모두 **생략할 수 있다**.

| 키워드 | 타입 | 의미 |
|---|---|---|
| `:key` | `(fn (A) A)` | 비교나 검사 전에 각 요소에 적용하는 사영 |
| `:test` | `(fn (A A) bool)` | `Eq` 경계의 `equals` 대신 쓰는 동등 검사. 첫 번째 인수는 **찾는 항목**, 두 번째는 요소(`:key` 적용 후)로, CL과 같은 순서 |
| `:test-not` | `(fn (A A) bool)` | `:test`의 부정 |
| `:start` `:end` | `int` | 훑을 창 `[start, end)`. 인덱스는 시퀀스 전체 기준 |
| `:from-end` | `bool` | 검색은 **마지막** 일치로 답한다. `:count`와 함께 쓰면 영향을 받는 요소를 끝에서부터 고른다 |
| `:count` | `int` | `remove` / `substitute` 계열이 영향을 주는 요소의 최대 개수 |

어느 함수가 어느 것을 받는지는 CL을 따른다.

| 함수 | 받는 키워드 |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | 위의 모든 것(`:count` 포함) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not`(`assoc`의 `:key`는 `car`에, `rassoc`의 것은 `cdr`에 적용된다) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; 끝에서부터 하나만 없앤다
(position 3 (iter v) :start 1)                          ; 인덱스는 시퀀스 전체 기준
```

**CL과의 차이**:

1. **`:key`의 사영은 요소 타입 안에 머문다**(`(fn (A) A)`). CL처럼 다른 타입으로 사영할 수는 없다. 인수를 생략했을 때 여분의
   타입 변수를 정할 수 없기 때문이다. 다른 타입으로의 사영이 필요하면 대신 `-if` 계열에 람다를 넘긴다
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **항목으로 찾는 검색에서 `:key`는 요소에만 적용된다**(찾는 항목에는 적용되지 않는다). CL의
   `find`/`position`/`count`/`member`/`remove`/`substitute`와 같은 규칙이다. 집합 연산에서는 양쪽 모두 요소이므로 양쪽에
   적용된다.
3. **`search`의 키워드만 번호가 아니라 이름으로 되어 있다.** CL에서 `:start1`/`:end1`은 **패턴**용, `:start2`/`:end2`는
   찾는 시퀀스용이다. 이 언어에서는 받는 쪽이 먼저 오므로 같은 번호가 반대 의미가 되며, 그것도 조용히 그렇게 된다.
   `:start`/`:end`는 받는 쪽, `:sub-start`/`:sub-end`는 패턴용이므로 무심코 쓴 `:start1`은 "알 수 없는 키워드" 오류가 된다.
   `mismatch`와 `replace`는 인수 순서가 CL과 같으므로 CL의 번호를 유지한다.

## 7. 파괴적 연산

`Vector<T>`의 메서드. **받는 쪽을 바꾸고 받는 쪽 자신을 반환하므로** `(nreverse v)`는 `reverse`와 같은 방식으로 쓰며 `v`
자신도 뒤집힌다.

| 이름 | 형식 | 설명 |
|---|---|---|
| `nreverse` | `(nreverse v)` | 제자리에서 뒤집는다 |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | `remove` / `remove-if` / `filter` / `remove-duplicates`의 제자리판 |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | `substitute` 계열의 제자리판 |
| `nbutlast` | `(nbutlast v)` | 마지막 요소를 버린다 |
| `fill` | `(fill v x)` | 모든 요소를 `x`로 한다. 길이는 바뀌지 않는다 |
| `replace` | `(replace v src)` | 앞에서부터 `src`의 요소로 덮어쓴다. `(min (len v) (len src))`개 |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. 개수는 위와 같다 |
| `nconc` | `(nconc v w)` | `w`의 요소를 `v`에 추가한다. CL과 달리 **공유 구조를 고쳐 쓰지 않는다**(`w`는 영향을 받지 않는다) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | `v`의 내용을 `src`로 바꾼다(길이도 바뀐다) |
| `rplaca` `rplacd` | `(rplaca p x)` | `cons-cell`의 `car`/`cdr`를 고쳐 쓰고 셀 자신을 반환한다 |

받는 키워드:

| 파괴판 | 받는 키워드 |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2`(받는 쪽이 CL의 `sequence-1`) |

`vector-push-extend`/`vector-pop`은 그냥 `Vector<T>`의 `push`/`pop`이다. `Vector<T>`는 항상 늘어나므로 CL의 "채움 포인터가
있는 벡터"와 "단순 벡터"의 구별에 해당하는 것은 없다.

## 8. 고차 함수

| 이름 | 형식 | 타입 | 설명 |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | 인수를 반환한다 |
| `const` | `(const x y)` | `(A,B)→A` | 첫 번째 인수를 반환한다 |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | 함수 합성 `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | 2인수 함수의 인수를 맞바꾼다 |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | 술어의 부정 |

CL의 `constantly`는 없다(무시되는 인수의 타입이 반환 타입에만 나타나 정할 수 없다). `(lambda ((x T)) A v)`라고 쓴다.
