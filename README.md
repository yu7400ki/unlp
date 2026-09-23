# unlp

UNLP（UnNatural Language Processing、不自然言語処理）は、AI（特に Claude）が書いた日本語の文章に残る癖を検出して、1000 字あたりの点として採点するコマンドラインツールです。コミットメッセージ、コードのコメントと文字列、README、設計文書を対象とし、エージェントが文章を出力する前の関門として使用することを想定しています。

検出器は検出と採点だけを担当します。指摘の修正方法については [skills/unlp/SKILL.md](skills/unlp/SKILL.md) と規則集 [skills/unlp/reference/rules.md](skills/unlp/reference/rules.md) に記述しており、エージェントのスキルとして読み込んで使用します。

## 導入

Rust のツールチェーン（edition 2024）が必要です。形態素解析の辞書（UniDic）をバイナリに同梱するため、ビルドには時間がかかります。

```bash
cargo install --path .
```

git の `commit-msg` hook として設置すると、コミットのたびにメッセージとステージした差分を採点して、しきい値を超過したコミットを拒否するようになります。

```bash
unlp hook install
```

設置の際に別の `commit-msg` hook が既にあれば、上書きせずに終了コード 1 で停止します。`unlp hook uninstall` で設置した hook を除去できます。

## 使い方

```bash
unlp check README.md src/            # ファイルとディレクトリ
unlp diff --staged                   # ステージした差分が触れた範囲
unlp diff HEAD~3..HEAD               # コミット範囲の差分
unlp commits -n 20                   # 直近 20 件のコミットメッセージ
unlp stdin < draft.txt               # 標準入力を 1 つの文書として
unlp stdin --format markdown < draft.md
unlp rules                           # 規則 ID、層、重み、規則集の見出し
```

共通のオプションは次のとおりです。

| オプション | 内容 |
|---|---|
| `--json` | 機械が読む形式で出力する |
| `--summary` | 指摘を省略して集計だけを出力する |
| `--fail-over <点>` | 点がこの値を超過したら終了コード 1 で終了する |
| `--config <path>` | 設定ファイルを指定する |

終了コードは、正常終了が 0、しきい値の超過が 1、入力や設定の誤りが 2 です。`--fail-over` を指定しない場合、指摘があっても終了コードは 0 です。

## 対象とする入力

| 種類 | 採点する範囲 |
|---|---|
| Markdown | 本文。コードブロック、インラインコード、HTML を除外する |
| ソースコード | コメントと文字列リテラル。Rust、Python、TypeScript、JavaScript、Go に対応し、Rust の `#[cfg(test)]` の項目を除外する |
| テキスト | 全体 |
| git | コミットメッセージと、差分で追加した行を含むコメントや本文 |

## 採点

規則は 22 件で、層ごとに ID を付けています。

| 層 | ID | 例 |
|---|---|---|
| 構造 | S01〜S06 | 無生物主語と発話動詞、文頭の指示代名詞、説明の足場語 |
| 語彙 | L01〜L03 | 慣習語の和語化、辞書の第一義による逐語訳、英語のリズムの直写 |
| 密度 | D01〜D04 | 短文の連打、述語の無い断片、無根拠の緩和、人間の文に普通の表現の欠如 |
| レジスター | R01〜R03 | 口語の混入、身体的な比喩、敬体と常体の混在 |
| 定型句 | F01〜F05 | 締めの定型、太字、2 倍ダッシュと矢印、空虚な強調 |
| 語種 | G01 | 文末の述語の和語率 |

点は、指摘の件数に規則の重みを乗じた合計を、日本語の文字数 1000 字あたりに正規化した値です。デフォルトのしきい値は 10 点です。日本語が 300 字に満たない入力は正規化せず、構造か語彙の層の指摘が 1 件でもあれば超過として扱います。

## 設定

対象のパスまたはカレントディレクトリから上位に走査して、最初に見つかった `unlp.toml` を読み込みます。

```toml
threshold = 10
floor = 300
exclude = ["tests/**", "examples/**"]

[weights]
S01 = 3.0

[lists.S01.person]
add = ["監督"]
remove = ["相手"]
```

しきい値、正規化の下限、除外するパスのグロブ、規則ごとの重み、語リストの追加と除外を指定できます。例えば上の設定では、S01 の重みを 3.0 にして、人を表す語のリストに `監督` を追加して `相手` を除外します。欄ごとの詳細は [skills/unlp/SKILL.md](skills/unlp/SKILL.md) に記載しています。デフォルトの重みは [data/weights.toml](data/weights.toml)、デフォルトの語リストは [data/lists](data/lists) にあります。

## 開発

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
