FROM public.ecr.aws/lambda/provided:al2023
COPY target/release/govscout-backend /var/task/bootstrap
