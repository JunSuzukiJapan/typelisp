<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# 모듈과 파일 구성

이 가이드에서는 여러 파일로 된 프로그램을 구성하는 방법을 설명한다. 자세한 규칙은
[문법 레퍼런스](../reference/syntax.md#310-module--use--이름공간)의 3.10~3.13절에 있다.

## 1. 파일 하나가 모듈 하나

typelisp에서는 **파일이 그 자체로 모듈**이다. 소스 루트에서 본 파일의 경로가 모듈의 경로가 된다.

| 파일 | 모듈 |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

파일 맨 앞에 모듈 선언을 쓸 필요는 없다.

## 2. 프로젝트 준비하기

프로젝트의 루트에 `typelisp.toml`이라는 파일을 둔다. 비어 있어도 된다.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

소스를 `src/` 아래에 두려면 `typelisp.toml`에 다음 한 줄을 쓴다.

```toml
src = "src"
```

`typl`은 실행하는 파일의 디렉터리에서 시작해 위로 올라가며 `typelisp.toml`을 찾고, 찾은 곳을 소스 루트로 쓴다. 찾지
못하면 실행하는 파일의 디렉터리가 루트가 된다(REPL에서는 현재 디렉터리).

## 3. 정의를 공개하고 사용하기

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; pub이 없는 필드는 밖에서 읽을 수 없다

(defun square ((n i32)) i32 (* n n))   ; pub이 없는 함수도 밖에서 호출할 수 없다

(pub defun dist2 ((a point) (b point)) i32
  (+ (square (- a::x b::x)) (square (- a::y b::y))))

(pub defmethod origin (point) point (point::new 0 0 "o"))
```

`main.typl`:

```lisp
(use geometry)

(let ((a (geometry::point::origin))
      (b (geometry::point::new 3 4 "b")))
  (println "~a" (geometry::dist2 a b)))
```

```sh
typl main.typl        # => 25
```

`geometry.typl`은 `(use geometry)`를 쓴 시점에 읽힌다. 미리 읽어 둘 필요는 없다.

### 공개되는 것

- 함수, 구조체, 열거형, 전역 변수, 매크로, 메서드는 `pub`이 붙어 있을 때만 다른 모듈에서 보인다. `(pub defun ...)`
  처럼 정의 바로 앞에 `pub`을 둔다.
- 구조체는 **타입의 공개와 필드의 공개가 따로**이다. `(pub defstruct point ...)`로 타입이 보이게 되고, `(pub x i32)`
  처럼 쓴 필드만 밖에서 읽고 쓸 수 있다.
- 공개되지 않은 이름을 밖에서 쓰면 `unresolved path: geometry::square` 같은 "해석할 수 없다"는 오류가 된다. 철자를
  틀렸을 때와 같은 메시지이므로, 철자가 맞는데도 해석되지 않으면 `pub`이 빠졌는지 의심한다.

`pub`을 붙일 수 있는 정의의 목록은 [문법 레퍼런스 3.13](../reference/syntax.md#313-pub--공개)에 있다.

## 4. `use` 쓰는 법

```lisp
(use geometry)              ; 모듈을 들여온다. geometry::dist2로 쓴다
(use geometry::dist2)       ; 함수를 들여온다. 맨 이름 dist2로 쓴다
(use geometry::point)       ; 타입을 들여온다. point::new, point::origin, 타입 표기의 point
(use a::f b::g)             ; 여러 개를 한꺼번에 쓸 수 있다
```

- **`use`는 그 뒤의 형식에만 효과가 있다.** 파일의 맨 앞에 쓴다. `use`보다 위에 `geometry::dist2`를 쓰면
  `unresolved path`가 된다.
- 모듈을 `use`하지 않고 전체 경로 `geometry::dist2`를 써도 해석되지 않는다. 파일을 읽게 하는 것은 `use`뿐이다.
- 맨 이름이 이미 쓰이고 있는 이름을 `use`하면 경고가 나온다. 그래도 들여오려면 `shadowing-import`를 쓴다.
- 디렉터리 안의 모듈은 `(use geo::shapes)`라고 쓰고, 그 뒤로는 마지막 부분(`shapes::...`)으로 가리킨다.

### 트레이트 메서드 호출하기

`impl`로 구현한 메서드는 모듈의 함수가 아니라 **타입에 속하므로** 모듈 이름 없이 호출한다.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; core::area가 아니라 area
```

`impl` 안의 메서드는 `pub`이 없어도 항상 공개된다.

트레이트 자체를 다른 모듈에 공개할 수는 없다. 트레이트의 정의, 그 `impl`, `:dyn`을 통해 그것을 쓰는 코드는 하나의
모듈에 둔다.

## 5. 파일 안에서 이름공간 나누기

하나의 파일 안에서 이름공간을 더 나누려면 `module`을 쓴다. 파일 자신의 모듈 안쪽에 중첩된다.

```lisp
;; main.typl 안
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

파일의 나머지 전체를 하나의 이름공간에 넣으려면 괄호로 감싸는 대신 `(in-module util)`이라고 쓸 수도 있다.

## 6. 의존 관계의 제한

- **순환은 허용되지 않는다.** `a.typl`이 `(use b)`하고 `b.typl`이 `(use a)`하면 `circular module dependency: a -> b
  -> a` 오류가 된다. 둘 다 필요로 하는 정의를 세 번째 모듈로 옮긴다.
- **타입도 함수도 정의되기 전에는 참조할 수 없다.** 같은 파일 안에서도 마찬가지이다. 상호 재귀하는 함수는 한쪽을
  `defsignature`로 먼저 선언한다([문법 레퍼런스 3.2](../reference/syntax.md#32-defsignature--전방-선언)).

## 7. 실행 순서

`typl main.typl`을 실행하면 다음 순서로 진행된다.

1. `main.typl`과 거기서 `use`된 모든 파일이 읽히고 타입 검사된다. **어딘가에 타입 오류가 있으면 아무것도 실행되지
   않는다.**
2. `use`된 모듈의 최상위 식이 그것을 쓰는 모듈의 것보다 먼저 실행된다.
3. `main.typl`의 최상위 식이 위에서부터 차례로 실행된다.

프로그램의 시작점을 `main` 함수에 모으고 파일 끝에서 `(main)`을 호출해 두면 같은 파일을
[AOT 컴파일](compile.md#3-aot-컴파일로-실행-파일-만들기)에도 쓸 수 있다.

## 8. `load`와의 차이

`(load "path")`는 Common Lisp의 `load`처럼 파일의 내용을 **현재 이름공간에 그대로** 읽어 들인다. 모듈로 감싸지
않으며 `pub`도 관계없다. 설정 파일을 읽거나 REPL에서 손 안의 파일을 다시 읽는 데 쓴다. 프로그램을 부품으로 나누려면
`use`를 쓴다.
