// Nimbus admin: theme toggle, select-all checkbox and delete confirmations. Optional; every page works without it.
(function () {
  var root = document.documentElement;
  var saved = localStorage.getItem('nimbus-admin-theme');
  if (saved === 'dark' || saved === 'light') root.dataset.theme = saved;
  document.addEventListener('DOMContentLoaded', function () {
    var toggle = document.querySelector('[data-theme-toggle]');
    if (toggle) {
      var dark = function () {
        return root.dataset.theme ? root.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
      };
      toggle.hidden = false;
      toggle.setAttribute('aria-pressed', String(dark()));
      toggle.addEventListener('click', function () {
        root.dataset.theme = dark() ? 'light' : 'dark';
        localStorage.setItem('nimbus-admin-theme', root.dataset.theme);
        toggle.setAttribute('aria-pressed', String(dark()));
      });
    }
    var all = document.querySelector('[data-select-all]');
    if (all) {
      all.addEventListener('change', function () {
        document.querySelectorAll('input[name="ids[]"]').forEach(function (box) { box.checked = all.checked; });
      });
    }
    document.querySelectorAll('form').forEach(function (form) {
      form.addEventListener('submit', function (event) {
        var message = form.dataset.confirm;
        var action = form.querySelector('select[name="action"]');
        if (action) message = action.selectedOptions[0] && action.selectedOptions[0].dataset.confirm;
        if (message && !confirm(message)) event.preventDefault();
      });
    });
  });
})();
