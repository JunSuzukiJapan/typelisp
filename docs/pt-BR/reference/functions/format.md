<!-- translated-from: docs/ja/reference/functions/format.md @ 7aae962c61e6f36448edc8bab6b37ff177aefdb3 -->
# Diretivas de formato

As diretivas escritas nas strings de controle de `print`/`println`/`format`. Elas cobrem quase todas as
diretivas do `format` do CL. As funções em si são descritas em
[Impressão](printing.md#1-print--println--format).

## 1. Como escrever diretivas

Cada diretiva é `~`, depois **parâmetros prefixos** opcionais (separados por vírgula: um inteiro / `'c` (um
caractere) / `v` (tirado do próximo argumento) / `#` (o número de argumentos restantes)), depois os
**modificadores** opcionais `:` e `@`, e depois o caractere da diretiva, nessa ordem. Os caracteres de
diretiva não diferenciam maiúsculas.

A string de controle deve ser um literal ([Impressão](printing.md#1-print--println--format)). Além disso, na
verificação são conferidos os pontos a seguir.

- **O número e os tipos dos argumentos.** Para cada diretiva que consome um argumento: se ainda resta um
  argumento e se seu tipo é aceito (as notas sobre "argumento" nas tabelas abaixo). Onde o caminho depende de
  valores em tempo de execução, como mover-se com `~*`, qual cláusula de `~[` é tomada, se `~^` dispara ou
  quantas vezes `~@{` se repete, **todos os caminhos** são verificados. Argumentos sobrando não são problema
  (como no CL).
- **Parâmetros e modificadores.** Um modificador não aceito, parâmetros demais e valores fora do intervalo (uma
  largura negativa, uma base fora de 2 a 36, um inteiro onde se espera um caractere etc.) são erros. Nunca são
  ignorados nem arredondados silenciosamente.

Os **elementos** de um argumento lista (`~{`, `~:{`, `~<...~:>`) são `Sexpr`, e nem seu número nem o tipo de
cada elemento podem ser conhecidos a partir dos tipos. As exigências sobre os elementos (um inteiro para
`~d` etc.) e os elementos que faltam são verificados quando os valores chegam, e são erros em tempo de
execução (nunca se troca por uma representação diferente no lugar).

As regras tolerantes do CL não são adotadas. Passar um não inteiro para `~d` e tê-lo impresso como `~a`, ou
`~:[` tratar qualquer valor como booleano, não são reinterpretados assim; são erros de tipo.

## 2. Saída (consome um argumento)

| Diretiva | Parâmetros / modificadores | Significado |
|---|---|---|
| `~a` | `~mincol,colinc,minpad,padchar` / `@`=alinhar à direita | Estética (o `princ` do CL; strings sem aspas). O argumento pode ser de qualquer tipo |
| `~s` | Igual ao acima | Padrão (o `prin1` do CL; uma forma que pode ser lida de volta). O argumento pode ser de qualquer tipo |
| `~w` | — | O `write` do CL. Faz impressão bonita se `*print-pretty*` for verdadeiro; senão, o mesmo que `~s` |
| `~d` `~b` `~o` `~x` | `~mincol,padchar,commachar,interval` / `:`=grupos de dígitos, `@`=sempre com sinal | Inteiros em decimal/binário/octal/hexadecimal. O argumento é um inteiro |
| `~r` | `~radix,mincol,padchar,commachar,interval` (com base) ou nenhum | Com base, essa base (2 a 36). Sem ela: `~r`=cardinal em inglês, `~:r`=ordinal em inglês, `~@r`=algarismos romanos, `~:@r`=algarismos romanos antigos. O argumento é um inteiro |
| `~p` | `:`=voltar um, `@`=y/ies | Plurais (`~p`→"s", `~@p`→"y"/"ies"). O argumento é um inteiro |
| `~c` | `:`=nome, `@`=sintaxe `#\` | Um caractere. O argumento é um `char` |
| `~f` | `~w,d,k,overflowchar,padchar` / `@`=sinal | Ponto fixo. O argumento é um número |
| `~e` | `~w,d,,,,padchar,exptchar` / `@`=sinal | Notação exponencial. O argumento é um número. Os parâmetros de dígitos do expoente, escala e overflowchar do CL não são suportados (informá-los é um erro) |
| `~g` | `@`=sinal | Ponto flutuante geral. O argumento é um número. Não recebe parâmetros |
| `~$` | `~d,n,w,padchar` / `:`,`@` | Notação monetária. O argumento é um número |

## 3. Saída (não consome argumentos)

| Diretiva | Significado |
|---|---|
| `~%` | Quebra de linha (`~n%` para n delas) |
| `~&` | fresh-line (uma quebra de linha, exceto no início de uma linha; `~n&`) |
| `~\|` | Quebra de página (form feed) |
| `~~` | Um `~` literal (`~n~` para n deles) |
| `~t` | Tabulação (`~colnum,colincT`. Se já estiver na coluna colnum ou além, avança um múltiplo de colinc; não se move se colinc for 0. `@`=relativa. `:`=uma tabulação relativa ao início do bloco lógico, que só funciona na impressão bonita) |
| `~_` | Quebra de linha condicional (pretty; simples=`:linear` / `~:_`=`:fill` / `~@_`=`:miser` / `~:@_`=`:mandatory`) |
| `~i` | Recuo (pretty; `~ni`=início do bloco + n / `~n:i`=coluna atual + n) |
| `~<newline>` | Ignora a quebra de linha (`:`=manter o espaço em branco, `@`=manter a quebra de linha) |

Como no CL, as diretivas do pretty printer (`~_` `~i` `~:t` `~<...~:>`, e o caminho de impressão bonita de
`~a`/`~s`/`~w`) não fazem nada quando `*print-pretty*` é falso. Por padrão ele é falso.

## 4. Estruturas de controle

| Diretiva | Significado |
|---|---|
| `~(...~)` | Conversão de maiúsculas (`~(` minúsculas, `~:(` inicial maiúscula em cada palavra, `~@(` inicial maiúscula só na primeira palavra, `~:@(` tudo em maiúsculas) |
| `~[...~;...~]` | Seleção condicional (ramifica conforme um argumento inteiro. Com `~n[`, `~v[` ou `~#[`, ramifica conforme esse valor e não pega argumento. `~:;`=a cláusula padrão, só como última cláusula). `~:[falso~;verdadeiro~]` ramifica conforme um argumento `bool` e tem exatamente duas cláusulas |
| `~{...~}` | Iteração (percorre um argumento lista. `~:{`=por sublista, `~@{`=sobre os argumentos restantes, `~:@{`=sobre cada lista entre os argumentos restantes, `~^`=sair, `~:}`=executar uma vez mesmo se vazio). Um corpo que não consome nenhum argumento em uma iteração é um erro (nunca terminaria) |
| `~<...~;...~>` | Justificação (distribui segmentos em `~mincol` colunas. `:`/`@`=preenchimento nas pontas) |
| `~<...~;...~:>` | **Bloco lógico** (fechado com `~:>`; é algo diferente da justificação acima). O primeiro segmento é o prefixo e o último é o sufixo (ambos só strings literais). Com o separador `~@;`, o prefixo é um **prefixo por linha**. `~:<` assume `(`/`)` como prefixo/sufixo padrão. O argumento é uma lista (`~@<` usa os argumentos restantes no lugar) |
| `~*` | Pular argumentos (`~n*`=avançar n, `~:*`=voltar, `~@*`=para uma posição absoluta) |
| `~/name/` | Chamada de função ou método (capítulo 5; as flags `:`/`@` são passadas para o alvo; não recebe parâmetros) |

As seguintes diretivas do CL não são suportadas (são erros na verificação).

- `~?` e `~@?`: recebem uma string de controle como argumento em tempo de execução, então não é possível
  verificar os argumentos que suas diretivas consomem. Escreva essas diretivas diretamente na string de
  controle.
- `~@[...~]`: testa se um argumento não é nil, mas esta linguagem não tem nil. Use `~:[falso~;verdadeiro~]`, que
  ramifica conforme um `bool`.
- `~{~}` com corpo vazio: tira o corpo de um argumento em tempo de execução. Escreva as diretivas dentro das
  chaves.

## 5. `~/name/`

`~/name/` resolve `name` pelas mesmas regras de uma chamada `(name arg colon at)` escrita nesse lugar e
emite a string que a chamada devolve. Consome um argumento. `colon` e `at` são `bool`s que dizem se a
diretiva tem `:` e `@`.

```lisp
(defstruct point (x i32) (y i32))
(impl point
  (brief (self (colon bool) (at bool)) string
    (if colon (format false "<~a,~a>" self::x self::y) (format false "~a/~a" self::x self::y))))
(defun yen ((n int) (colon bool) (at bool)) string
  (format false "~a yen" n))

(println "~a" (format false "~/brief/"  (make-instance 'point :x 3 :y 4)))   ; => 3/4
(println "~a" (format false "~:/brief/" (make-instance 'point :x 3 :y 4)))   ; => <3,4>
(println "~a" (format false "~/yen/" 500))                                    ; => 500 yen
```

- A resolução do nome é a mesma de uma chamada ([Referência de sintaxe 3.10](../syntax.md#310-module--use--namespaces)). Se o tipo do
  argumento tem um método `name`, ele é chamado; senão, a função visível `name`. Uma função de um módulo
  pode ser nomeada pelo seu caminho (`~/m::shout/`).
- O que pode ser chamado é uma função ou um método escrito em typelisp que recebe o argumento, `colon` e
  `at` como estão e devolve uma `string`. Funções embutidas não podem ser chamadas.
- **O que chamar é decidido na verificação.** O verificador encontra todos os argumentos que podem chegar
  a esse `~/name/`, levando em conta os movimentos de `~*` e `~[`, e resolve o nome com o tipo de cada um.
  Se algum não puder ser resolvido, é um erro na verificação, e também é erro o caso em que os argumentos
  que podem chegar chamariam duas definições diferentes. Nenhum nome é procurado em tempo de execução,
  então funciona igual nos executáveis AOT.
- **Não pode ser usado dentro de `~{`, `~:{`, `~:@{` ou `~<...~:>`** (erro na verificação). O tipo dos
  elementos de um argumento lista é `Sexpr`, que não decide o que chamar. `~@{` percorre os argumentos
  restantes da chamada, cujos tipos são conhecidos, então ali pode ser usado.

A diferença do CL: o CL procura o nome como função global. Aqui a resolução é a mesma de uma chamada, então
um método do tipo do argumento é escolhido antes de uma função de mesmo nome.
