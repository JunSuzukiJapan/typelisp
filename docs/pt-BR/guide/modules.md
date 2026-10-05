<!-- translated-from: docs/ja/guide/modules.md @ 5a8204d1f6a60c462be82bf95b46e9dc1987d572 -->
# Módulos e organização de arquivos

Este guia explica como montar um programa composto por vários arquivos. As regras detalhadas estão nas
seções 3.10 a 3.13 da [Referência de sintaxe](../reference/syntax.md#310-module--use--namespaces).

## 1. Um arquivo é um módulo

No typelisp, **um arquivo é um módulo por si só**. O caminho do arquivo relativo à raiz dos fontes é o
caminho do módulo.

| Arquivo | Módulo |
|---|---|
| `<root>/geometry.typl` | `geometry` |
| `<root>/geo/shapes.typl` | `geo::shapes` |
| `<root>/net/http/client.typl` | `net::http::client` |

Não é preciso escrever uma declaração de módulo no início do arquivo.

## 2. Preparar um projeto

Coloque um arquivo chamado `typelisp.toml` na raiz do projeto. Ele pode estar vazio.

```
myapp/
├── typelisp.toml
├── main.typl
└── geometry.typl
```

Para manter os fontes em `src/`, escreva esta linha em `typelisp.toml`:

```toml
src = "src"
```

O `typl` procura `typelisp.toml` a partir do diretório do arquivo que executa, subindo, e usa o lugar onde o
encontra como raiz dos fontes. Se não encontrar nenhum, a raiz é o diretório do arquivo executado (no REPL,
o diretório atual).

## 3. Tornar definições públicas e usá-las

`geometry.typl`:

```lisp
(pub defstruct point
  (pub x i32)
  (pub y i32)
  (tag string))                        ; um campo sem pub não pode ser lido de fora

(defun square ((n i32)) i32 (* n n))   ; uma função sem pub também não pode ser chamada de fora

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

`geometry.typl` é carregado no ponto em que `(use geometry)` está escrito. Não é preciso carregá-lo antes.

### O que se torna público

- Funções, estruturas, enumerações, variáveis globais, macros e métodos são visíveis de outros módulos só
  quando levam `pub`. Coloque `pub` logo antes da definição, como em `(pub defun ...)`.
- Nas estruturas, **tornar o tipo público e tornar os campos públicos são coisas separadas**.
  `(pub defstruct point ...)` torna o tipo visível, e só os campos escritos como `(pub x i32)` podem ser lidos
  e escritos de fora.
- Usar de fora um nome que não é público dá um erro de "não é possível resolver" como
  `unresolved path: geometry::square`. É a mesma mensagem de um nome digitado errado, então se a grafia
  estiver certa e o nome ainda não se resolver, suspeite de um `pub` faltando.

A lista de definições que podem levar `pub` está em
[Referência de sintaxe 3.13](../reference/syntax.md#313-pub--visibilidade).

## 4. Como escrever `use`

```lisp
(use geometry)              ; trazer um módulo; para usá-lo escreve-se geometry::dist2
(use geometry::dist2)       ; trazer uma função; usa-se pelo nome simples dist2
(use geometry::point)       ; trazer um tipo; point::new, point::origin e point nas anotações de tipo
(use a::f b::g)             ; vários podem ser escritos juntos
```

- **`use` só vale para as formas que vêm depois.** Coloque-o no início do arquivo. Escrever `geometry::dist2`
  acima do `use` dá `unresolved path`.
- Escrever o caminho completo `geometry::dist2` sem fazer `use` do módulo também não se resolve. Só o `use`
  faz um arquivo ser carregado.
- Fazer `use` de um nome cuja forma simples já está em uso dá um aviso. Quando quiser trazê-lo mesmo assim,
  use `shadowing-import`.
- Um módulo dentro de um diretório é escrito `(use geo::shapes)`, e daí em diante é referido pela última parte
  (`shapes::...`).

### Chamar métodos de traits

Os métodos implementados em um `impl` **pertencem ao tipo**, não às funções do módulo, então são chamados sem
o nome do módulo.

```lisp
(use shapes::core)
(println "~a" (area (core::square::new 4)))    ; area, não core::area
```

Os métodos dentro de um `impl` são sempre públicos, mesmo sem `pub`.

Um trait em si não pode ser tornado público para outros módulos. Mantenha a definição de um trait, seus
`impl` e o código que o usa via `:dyn` em um único módulo.

## 5. Dividir namespaces dentro de um arquivo

Para dividir ainda mais um namespace dentro de um arquivo, use `module`. Ele fica aninhado dentro do módulo
do próprio arquivo.

```lisp
;; dentro de main.typl
(module util
  (pub defun clamp ((x i32) (lo i32) (hi i32)) i32
    (if (< x lo) lo (if (> x hi) hi x))))

(println "~a" (util::clamp 15 0 10))    ; main::util::clamp
```

Para colocar todo o resto do arquivo em um namespace, você pode escrever `(in-module util)` em vez de
envolvê-lo em parênteses.

## 6. Restrições de dependência

- **Ciclos não são permitidos.** Se `a.typl` faz `(use b)` e `b.typl` faz `(use a)`, o resultado é o erro
  `circular module dependency: a -> b -> a`. Mova as definições de que ambos precisam para um terceiro
  módulo.
- **Nem tipos nem funções podem ser referenciados antes de serem definidos**, mesmo dentro do mesmo arquivo.
  Para funções mutuamente recursivas, declare uma delas primeiro com `defsignature`
  ([Referência de sintaxe 3.2](../reference/syntax.md#32-defsignature--declarações-antecipadas)).

## 7. Ordem de execução

Executar `typl main.typl` segue esta ordem:

1. `main.typl` e todos os arquivos usados a partir dele com `use` são lidos e têm os tipos verificados. **Se
   houver um erro de tipo em qualquer lugar, nada é executado.**
2. As expressões de nível superior dos módulos usados são executadas antes das dos módulos que os usam.
3. As expressões de nível superior de `main.typl` são executadas de cima para baixo.

Se você reunir o ponto de entrada do programa em uma função `main` e chamar `(main)` no final do arquivo, o
mesmo arquivo também serve para a
[compilação AOT](compile.md#3-construir-um-executável-com-compilação-aot).

## 8. Em que difere de `load`

`(load "caminho")`, como o `load` do Common Lisp, lê o conteúdo de um arquivo **no namespace atual, tal como
está**. Ele não o envolve em um módulo, e `pub` não tem papel nenhum. Use-o para coisas como ler um arquivo
de configuração ou recarregar um arquivo local no REPL. Para dividir um programa em partes, use `use`.
