// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
export function register() {
  return {
    // The SQLite worker opens its file handles during page startup.
    day_plan_retry_storage() { location.reload(); },
  };
}
