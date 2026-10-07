<!-- translated-from: docs/ja/reference/functions/option-result.md @ 3093a4f5a38833618b09ebc46584252a999a384e -->
# Option, Result en foutentypes

## 1. `Option<T>` / `Result<T,E>`

Constructors: `Option<T>` heeft `Some(T)` / `None`. `Result<T,E>` heeft `Ok(T)` / `Err(E)`. `E` kan
elk type zijn: de ingebouwde concrete foutentypes, en types die je zelf met `defstruct`/`defenum`
schrijft, passen er net zo goed (hoofdstuk 3).

| Naam | Vorm | Option | Result | Beschrijving |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Haalt de waarde eruit. Geeft een panic bij `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | De waarde, of de standaardwaarde |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Of het `Some` is |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Of het `None` is |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Of het `Ok` is |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Of het `Err` is |

De constructors zijn `Option::some`/`Option::none`/`Result::ok`/`Result::err` (of, na
`(use option)`/`(use result)`, de kale namen `some`/`none`/`ok`/`err`).

Vertakken schrijf je expliciet met `match`. Er is geen syntaxis die met `?` van Rust overeenkomt.

## 2. De runtime-representatie van `Option<T>`

Net als in Rust **maakt `Option<T>` meestal geen box**. `some v` is `v` zelf en `none` is de waarde
van de lege lijst, zonder allocatie en zonder indirectie. `Option<Sexpr>` (waarbij de lege lijst
`none` is), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` en
`Option<(fn ...)>` hebben allemaal deze vorm.

Er wordt alleen een box gebruikt wanneer een waarde van `T` niet van de waarde van de lege lijst kan
worden onderscheiden:

| `T` | Representatie | Reden |
|---|---|---|
| `Option<U>` (genest) | Box | De binnenste `none` zou dezelfde waarde zijn als de buitenste `none` |
| `()` | Box | De waarde van `()` is zelf de waarde van de lege lijst |
| `ptr` / `c-long` / `c-ulong` | Box | Alle 64 bits zijn waarde, zodat er geen ruimte is om ze te onderscheiden |
| Al het andere | Geen box | — |

De representatie wordt door het type alleen bepaald en kan niet uit een waarde worden gelezen. Bij het
afdrukken wordt `(some ...)`/`none` uit het statische type gereconstrueerd, dus
`(format false "~a" opt)` drukt `(some 1)` af. Er zijn twee beperkingen:

- **Het kan niet in een `:dyn Trait` worden gezet** (een waarde van `Option<int>` waarvoor je
  `(impl Speak Option<int> ...)` schreef doorgeven aan een `:dyn Speak` is een fout).
- Een downcast `(the Option<T> ...)` vanuit een `Sexpr` **noemt een constructor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. De vorm die de hele waarde bindt,
  `(the Option<int> o)`, is een fout.

## 3. Foutentypes en de trait `Error`

Naar het voorbeeld van `std::error::Error` van Rust **is `Error` geen type maar een trait**. De
concrete types die fouten voorstellen zijn per doel gescheiden, en elk implementeert `Error`.

| Type | Geproduceerd door |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Bestands- en streambewerkingen ([Streams en bestanden](streams-files.md)) |
| `NetError` | Netwerkbewerkingen ([Netwerk](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. `simple-error` van CL: de standaardkeuze wanneer je alleen wilt zeggen wat er gebeurde |
| `WrappedError` | `(wrap-error msg cause)`. Een type dat zowel je eigen melding als de oorzaak draagt; het is de reden dat de trait `Error` `source` heeft |

`ParseIntError` tot en met `NetError` zijn elk "een enum met één enkele variant die één
meldingsstring bevat", en de typenaam en de variantnaam zijn dezelfde
(`(match e ((ParseIntError m) m))`, gebouwd met `(ParseIntError::ParseIntError "...")`). Er is niets
bijzonders aan: ze worden precies zo behandeld als je eigen foutentypes die met
`(defstruct my-err (...))` / `(defenum my-err ...)` zijn geschreven.

| Naam | Vorm | Type | Beschrijving |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | De foutmelding (een methode van de trait `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | De oorzaak die deze fout omhult, of `None` als er geen is (`Error::source` van Rust) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementeert `Error`) | Verbreedt een concreet foutentype tot het trait-object |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementeert `Error`) | De melding en de keten van oorzaken die je vindt door `source` te volgen, één oorzaak per regel. CL heeft geen tegenhanger ("caused by" van Rust) |

Als je `Error` voor je eigen foutentype implementeert, kan het **op dezelfde manier** als de
ingebouwde fouten worden behandeld:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; the concrete type goes into E as it is
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; handle any kind uniformly
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

Om meerdere foutentypes in één `Result` te verzamelen, gebruik je `Result<T, :dyn Error>` (overeenkomend
met `Box<dyn Error>` van Rust), en verbreed je concrete fouten met `as-dyn-error`. Omdat er geen `?`
is, wordt deze conversie expliciet geschreven:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Types en traits delen één namespace** (zoals in Rust). Binnen één module kunnen een
`defstruct`/`defenum` en een trait niet dezelfde naam hebben, en een traitnaam op een typeplek
schrijven wordt gemeld als "`error` is a trait, not a type — write `:dyn error`".

Niet-herstelbare fouten worden met `panic` uitgedrukt. Voor `panic` en `catch`/`throw` zie de
[Syntaxreferentie](../syntax.md#8-niet-lokale-uitgangen-catch--throw--unwind-protect); voor het
foutafhandelingsbeleid zie [hoofdstuk 9 daarvan](../syntax.md#9-foutafhandelingsbeleid).
