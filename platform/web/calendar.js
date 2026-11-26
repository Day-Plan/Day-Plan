// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
// Keep a civil due date on the user's local day, including across daylight-saving changes.
export function register() {
  return {
    day_plan_local_date() {
      const now = new Date();
      return now.getFullYear() * 10000 + (now.getMonth() + 1) * 100 + now.getDate();
    },
  };
}
