data "aws_caller_identity" "current" {}

resource "aws_s3_bucket" "bucket" {
    bucket = "${var.name}-data.aws_caller_identity.current.account_id"
    acl = "public-read"
}


