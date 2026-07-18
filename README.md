# 레트로 게임 한글 패치 프로젝트 템플릿

저작 자산을 저장소에서 제외하고, 자유로운 조사와 재현 가능한 제품
빌드를 분리하며, 위험한 이진 쓰기를 검증하기 위한 시작점이다. 플랫폼,
구현 언어, CLI와 데이터 형식은 대상 조사 뒤 프로젝트가 정한다.

한글화 판단 방법은
[create-retro-game-kr-patch](https://github.com/mcpads/create-retro-game-kr-patch)가
설명한다. 이 템플릿은 반복 빌드에서 기계적으로 막을 수 있는 실패를
구현한다.

## 사용

새 프로젝트를 만들 때의 시작 규칙은 [`AGENTS.md`](AGENTS.md)를 따른다.
언어 중립 반례는
[`conformance/`](conformance/), Rust 참고 구현은
[`reference/rust/`](reference/rust/)에 있다.

번역과 글꼴 자산을 채택할 때는 [`assets/translation/`](assets/translation/)과
[`assets/fonts/`](assets/fonts/)의 입력 규칙을 참고한다. 실행 검증에
emucap을 선택하면 [`adapters/emucap/`](adapters/emucap/)의 연결 규칙을
적용한다.

## 참고 구현 확인

```bash
cd reference/rust
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

이 템플릿은 원격 CI 설정을 포함하지 않는다.
