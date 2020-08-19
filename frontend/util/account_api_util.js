
export const fetchAccounts = () => (
  $.ajax({
    url: '/api/accounts'
  })
);

export const createAccount = account => (
  $.ajax({
    url: 'api/accounts',
    method: 'POST',
    data: { account }
  })
);

export const updateAccount = account => (
  $.ajax({
    url: `api/accounts/${account.id}`,
    method: 'PATCH',
    data: { account }
  })
);

export const deleteAccount = accountId => (
  $.ajax({
    url: `api/accounts/${accountId}`,
    method: 'DELETE'
  })
);