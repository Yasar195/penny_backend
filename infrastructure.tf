terraform {
  required_version = ">= 1.5.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    null = {
      source  = "hashicorp/null"
      version = "~> 3.2"
    }
  }
}

provider "aws" {
  region = var.aws_region
}

data "aws_caller_identity" "current" {}

locals {
  ecr_registry = "${data.aws_caller_identity.current.account_id}.dkr.ecr.${var.aws_region}.amazonaws.com"
  image_uri    = "${aws_ecr_repository.app.repository_url}:${var.image_tag}"
  lambda_env = merge(
    {
      PORT           = "8080"
      ROCKET_ADDRESS = "0.0.0.0"
      ROCKET_PORT    = "8080"
    },
    var.lambda_environment,
    var.database_url == null ? {} : { DATABASE_URL = var.database_url }
  )

  tags = {
    Project     = var.project_name
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

variable "aws_region" {
  type        = string
  description = "AWS region for all resources."
  default     = "ap-south-1"
}

variable "project_name" {
  type        = string
  description = "Project tag/name prefix."
  default     = "penny"
}

variable "environment" {
  type        = string
  description = "Environment tag."
  default     = "prod"
}

variable "ecr_repository_name" {
  type        = string
  description = "ECR repository name."
  default     = "penny-lambda"
}

variable "image_tag" {
  type        = string
  description = "Container image tag to push and deploy."
  default     = "latest"
}

variable "lambda_function_name" {
  type        = string
  description = "Lambda function name."
  default     = "penny-prod"
}

variable "docker_platform" {
  type        = string
  description = "Docker build platform."
  default     = "linux/amd64"
}

variable "lambda_memory_size" {
  type        = number
  description = "Lambda memory size in MB."
  default     = 1024
}

variable "lambda_timeout" {
  type        = number
  description = "Lambda timeout in seconds."
  default     = 30
}

variable "log_retention_days" {
  type        = number
  description = "CloudWatch log retention in days."
  default     = 14
}

variable "lambda_environment" {
  type        = map(string)
  description = "Additional Lambda environment variables (for example DATABASE_URL)."
  default     = {}
}

variable "database_url" {
  type        = string
  description = "PostgreSQL connection URL passed to Lambda as DATABASE_URL."
  default     = null
  sensitive   = true
  nullable    = true
}

variable "create_function_url" {
  type        = bool
  description = "Create a public Lambda Function URL (optional when using API Gateway)."
  default     = false
}

variable "create_api_gateway" {
  type        = bool
  description = "Create an API Gateway HTTP API in front of Lambda."
  default     = true
}

variable "api_gateway_name" {
  type        = string
  description = "API Gateway HTTP API name. If null, uses <lambda_function_name>-http-api."
  default     = null
  nullable    = true
}

resource "aws_ecr_repository" "app" {
  name                 = var.ecr_repository_name
  image_tag_mutability = "MUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }

  tags = merge(local.tags, {
    Name = var.ecr_repository_name
  })
}

resource "aws_ecr_lifecycle_policy" "keep_recent_images" {
  repository = aws_ecr_repository.app.name

  policy = jsonencode({
    rules = [
      {
        rulePriority = 1
        description  = "Keep last 20 images"
        selection = {
          tagStatus   = "any"
          countType   = "imageCountMoreThan"
          countNumber = 20
        }
        action = {
          type = "expire"
        }
      }
    ]
  })
}

resource "aws_iam_role" "lambda_exec" {
  name = "${var.lambda_function_name}-exec-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Principal = {
          Service = "lambda.amazonaws.com"
        }
        Action = "sts:AssumeRole"
      }
    ]
  })

  tags = local.tags
}

resource "aws_iam_role_policy_attachment" "lambda_basic_execution" {
  role       = aws_iam_role.lambda_exec.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"
}

resource "aws_cloudwatch_log_group" "lambda" {
  name              = "/aws/lambda/${var.lambda_function_name}"
  retention_in_days = var.log_retention_days
  tags              = local.tags
}

resource "null_resource" "build_and_push_image" {
  triggers = {
    image_uri       = local.image_uri
    dockerfile_hash = filesha256("${path.module}/Dockerfile")
    cargo_toml_hash = filesha256("${path.module}/Cargo.toml")
    cargo_lock_hash = filesha256("${path.module}/Cargo.lock")
    src_hash = sha256(join(
      "",
      [for file in sort(fileset(path.module, "src/**")) : filesha256("${path.module}/${file}")]
    ))
  }

  depends_on = [aws_ecr_repository.app]

  provisioner "local-exec" {
    interpreter = ["/bin/bash", "-lc"]
    command     = <<-EOT
      set -euo pipefail
      aws ecr get-login-password --region '${var.aws_region}' | docker login --username AWS --password-stdin '${local.ecr_registry}'
      docker buildx build --platform '${var.docker_platform}' -t '${local.image_uri}' --push .
    EOT
  }
}

resource "aws_lambda_function" "app" {
  function_name = var.lambda_function_name
  role          = aws_iam_role.lambda_exec.arn
  package_type  = "Image"
  image_uri     = local.image_uri
  memory_size   = var.lambda_memory_size
  timeout       = var.lambda_timeout
  architectures = ["x86_64"]

  environment {
    variables = local.lambda_env
  }

  lifecycle {
    precondition {
      condition     = can(local.lambda_env.DATABASE_URL) && trimspace(local.lambda_env.DATABASE_URL) != ""
      error_message = "DATABASE_URL is required. Set var.database_url or include DATABASE_URL in var.lambda_environment."
    }
  }

  depends_on = [
    null_resource.build_and_push_image,
    aws_iam_role_policy_attachment.lambda_basic_execution,
    aws_cloudwatch_log_group.lambda
  ]

  tags = local.tags
}

resource "aws_apigatewayv2_api" "app" {
  count         = var.create_api_gateway ? 1 : 0
  name          = coalesce(var.api_gateway_name, "${var.lambda_function_name}-http-api")
  protocol_type = "HTTP"

  cors_configuration {
    allow_credentials = false
    allow_origins     = ["*"]
    allow_methods     = ["*"]
    allow_headers     = ["*"]
    expose_headers    = ["date", "keep-alive"]
    max_age           = 86400
  }

  tags = local.tags
}

resource "aws_apigatewayv2_integration" "app_lambda" {
  count                  = var.create_api_gateway ? 1 : 0
  api_id                 = aws_apigatewayv2_api.app[0].id
  integration_type       = "AWS_PROXY"
  integration_method     = "POST"
  integration_uri        = aws_lambda_function.app.invoke_arn
  payload_format_version = "2.0"
  timeout_milliseconds   = 30000
}

resource "aws_apigatewayv2_route" "root" {
  count     = var.create_api_gateway ? 1 : 0
  api_id    = aws_apigatewayv2_api.app[0].id
  route_key = "ANY /"
  target    = "integrations/${aws_apigatewayv2_integration.app_lambda[0].id}"
}

resource "aws_apigatewayv2_route" "proxy" {
  count     = var.create_api_gateway ? 1 : 0
  api_id    = aws_apigatewayv2_api.app[0].id
  route_key = "ANY /{proxy+}"
  target    = "integrations/${aws_apigatewayv2_integration.app_lambda[0].id}"
}

resource "aws_apigatewayv2_stage" "default" {
  count       = var.create_api_gateway ? 1 : 0
  api_id      = aws_apigatewayv2_api.app[0].id
  name        = "$default"
  auto_deploy = true

  tags = local.tags
}

resource "aws_lambda_permission" "allow_apigw_invoke" {
  count         = var.create_api_gateway ? 1 : 0
  statement_id  = "AllowExecutionFromAPIGateway"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.app.function_name
  principal     = "apigateway.amazonaws.com"
  source_arn    = "${aws_apigatewayv2_api.app[0].execution_arn}/*/*"
}

resource "aws_lambda_function_url" "app" {
  count              = var.create_function_url ? 1 : 0
  function_name      = aws_lambda_function.app.function_name
  authorization_type = "NONE"

  cors {
    allow_credentials = false
    allow_origins     = ["*"]
    allow_methods     = ["*"]
    allow_headers     = ["*"]
    expose_headers    = ["date", "keep-alive"]
    max_age           = 86400
  }
}

resource "aws_lambda_permission" "public_function_url" {
  count                  = var.create_function_url ? 1 : 0
  statement_id           = "AllowPublicFunctionUrlInvoke"
  action                 = "lambda:InvokeFunctionUrl"
  function_name          = aws_lambda_function.app.function_name
  principal              = "*"
  function_url_auth_type = "NONE"
}

output "ecr_repository_url" {
  value       = aws_ecr_repository.app.repository_url
  description = "ECR repository URL."
}

output "lambda_function_name" {
  value       = aws_lambda_function.app.function_name
  description = "Lambda function name."
}

output "image_uri" {
  value       = local.image_uri
  description = "Deployed image URI."
}

output "lambda_function_url" {
  value       = var.create_function_url ? aws_lambda_function_url.app[0].function_url : null
  description = "Public function URL (if enabled)."
}

output "api_gateway_invoke_url" {
  value       = var.create_api_gateway ? aws_apigatewayv2_stage.default[0].invoke_url : null
  description = "API Gateway HTTP API invoke URL (if enabled)."
}
