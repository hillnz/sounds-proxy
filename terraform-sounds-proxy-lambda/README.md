# terraform-sounds-proxy-lambda

This terraform module deploys sounds-proxy to AWS Lambda.

It creates and configures an S3 bucket that episodes will be served from, and configures a public Lambda function url. Cached episodes are automatically expired from the S3 bucket.

## Deploy

You need these locally
- docker
- terraform
- aws cli, and credentials configured

Then you should be able to `terraform apply`.
Optionally, you can set the `name` [input variable](https://www.terraform.io/language/values/variables#assigning-values-to-root-module-variables), or it'll default to `sounds-proxy`.

After apply, the Lambda's function url will be output.

## Caveats

With low traffic, it should be extremely cheap to run. 

With high traffic, it could get expensive because episodes are served directly from S3. You'd be best to use this terraform module in conjunction with a CDN e.g. CloudFront/CloudFlare (this would also let you customise the URL).
