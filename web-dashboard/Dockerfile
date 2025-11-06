# Serve pre-built React application with nginx
FROM nginx:alpine

# Copy pre-built assets (build outside Docker)
COPY dist /usr/share/nginx/html

# Copy custom nginx configuration
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Expose port 80
EXPOSE 80

# Start nginx
CMD ["nginx", "-g", "daemon off;"]
