terraform {
  required_providers {
    docker = {
      source  = "kreuzwerker/docker"
      version = "2.16.0"
    }
  }
}

data "aws_caller_identity" "current" {}

data "aws_region" "current" {}

# Bucket
# ‾‾‾‾‾‾

# Where episodes are (temporarily) stored

resource "aws_s3_bucket" "bucket" {
  bucket = "${var.name}-${data.aws_caller_identity.current.account_id}"
}

resource "aws_s3_bucket_acl" "bucket_acl" {
  bucket = aws_s3_bucket.bucket.id
  acl    = "public-read"
}

resource "aws_s3_bucket_lifecycle_configuration" "bucket_lifecycle" {
  bucket = aws_s3_bucket.bucket.id

  rule {
    id     = "expire-all-files"
    status = "Enabled"

    expiration {
      days = 7
    }
  }

  rule {
    id     = "abort-incomplete-uploads"
    status = "Enabled"

    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }
}

# Image
# ‾‾‾‾‾

# Annoyingly, lambda can't use public ECR images so this mirrors it to a private repo

locals {
  image_registry = "${data.aws_caller_identity.current.account_id}.dkr.ecr.${data.aws_region.current.id}.amazonaws.com"
}

data "aws_ecr_authorization_token" "token" {}

provider "docker" {
  registry_auth {
    address  = local.image_registry
    username = data.aws_ecr_authorization_token.token.user_name
    password = data.aws_ecr_authorization_token.token.password
  }
}

data "local_file" "dockerfile" {
  filename = abspath("${path.module}/image/Dockerfile")
}

module "docker_image" {
  source = "terraform-aws-modules/lambda/aws//modules/docker-build"

  create_ecr_repo = true
  ecr_repo        = "${var.name}-lambda"
  # Pulls tag from Dockerfile
  image_tag   = regex("(?m:^FROM.+:(.+)$)", data.local_file.dockerfile.content)[0]
  source_path = abspath("${path.module}/image")

  ecr_repo_lifecycle_policy = jsonencode({
    "rules" : [
      {
        "rulePriority" : 1,
        "description" : "Expire old versions",
        "selection" : {
          "tagStatus" : "any",
          "countType" : "imageCountMoreThan",
          "countNumber" : 2,
        },
        "action" : {
          "type" : "expire"
        }
      }
    ]
  })
}

# Lambda
# ‾‾‾‾‾‾

module "lambda_function_container_image" {
  source = "terraform-aws-modules/lambda/aws"

  function_name = var.name

  create_lambda_function_url = true
  timeout                    = 30

  create_package = false
  package_type   = "Image"
  image_uri      = module.docker_image.image_uri

  environment_variables = {
    "SOUNDS_PROXY_S3_BUCKET" = aws_s3_bucket.bucket.id
  }

  attach_policy_statements = true
  policy_statements = {
    s3_list = {
      effect = "Allow",
      actions = [
        "s3:GetBucketLocation",
        "s3:ListBucket"
      ],
      resources = [
        aws_s3_bucket.bucket.arn
      ]
    },
    s3_readwrite = {
      effect = "Allow",
      actions = [
        "s3:GetObject",
        "s3:PutObject",
      ],
      resources = [
        "${aws_s3_bucket.bucket.arn}/*"
      ]
    },
  }

}
