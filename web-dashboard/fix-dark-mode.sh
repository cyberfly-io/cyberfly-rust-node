#!/bin/bash

# This script properly adds dark mode classes to all React components
# It restores light mode defaults and adds dark: variant classes

cd /home/cyberfly/cyberfly-rust-node/web-dashboard/src/components

# Fix backgrounds - restore white background and add dark variant
find . -name "*.tsx" -exec sed -i \
  -e 's/className="\([^"]*\)bg-gray-800\([^"]*\)"/className="\1bg-white dark:bg-gray-800\2"/g' \
  -e 's/className="\([^"]*\)bg-gray-700\([^"]*\)"/className="\1bg-gray-50 dark:bg-gray-700\2"/g' \
  {} \;

# Fix text colors - restore dark text and add light text for dark mode
find . -name "*.tsx" -exec sed -i \
  -e 's/className="\([^"]*\)text-gray-100\([^"]*\)"/className="\1text-gray-900 dark:text-gray-100\2"/g' \
  -e 's/className="\([^"]*\)text-gray-200\([^"]*\)"/className="\1text-gray-800 dark:text-gray-200\2"/g' \
  -e 's/className="\([^"]*\)text-gray-300\([^"]*\)"/className="\1text-gray-700 dark:text-gray-300\2"/g' \
  -e 's/className="\([^"]*\)text-gray-400\([^"]*\)"/className="\1text-gray-600 dark:text-gray-400\2"/g' \
  {} \;

# Fix borders - add dark mode border colors
find . -name "*.tsx" -exec sed -i \
  -e 's/className="\([^"]*\)border-gray-200\([^"]*\)"/className="\1border-gray-200 dark:border-gray-700\2"/g' \
  -e 's/className="\([^"]*\)border-gray-300\([^"]*\)"/className="\1border-gray-300 dark:border-gray-600\2"/g' \
  {} \;

# Fix hover states - add dark mode hover
find . -name "*.tsx" -exec sed -i \
  -e 's/hover:bg-gray-100\([^"]*\)"/hover:bg-gray-100 dark:hover:bg-gray-700\1"/g' \
  -e 's/hover:bg-gray-50\([^"]*\)"/hover:bg-gray-50 dark:hover:bg-gray-600\1"/g' \
  {} \;

echo "Dark mode classes added successfully to all components!"
