# govscout-backend
AWS Lambda function logic for scouring government websites for contracting opportunities.

This includes the Soup crate by Paul Woolcock at [commit 23c67206](https://gitlab.com/pwoolcoc/soup/-/tree/23c67206f62d0dfeb790c6f438951adb95c1a69d),
with adapatations made to use the latest (0.27, 0.3) versions of [html5ever](https://docs.rs/html5ever/latest/html5ever/) and [markup5ever_rcdom](https://docs.rs/markup5ever_rcdom/latest/markup5ever_rcdom/)