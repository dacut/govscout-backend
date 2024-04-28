FROM public.ecr.aws/lambda/provided:al2023
COPY target/debug/govscout-backend /var/runtime/bootstrap
CMD ["main"]