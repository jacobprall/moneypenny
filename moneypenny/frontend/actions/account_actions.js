import * as AccountAPIUtil from '../util/account_api_util'


export const RECEIVE_ACCOUNTS = 'RECEIVE_ACCOUNTS';
export const RECEIVE_ACCOUNT = 'RECEIVE_ACCOUNT';
export const REMOVE_ACCOUNT = 'REMOVE_ACCOUNT';

export const receiveAccounts = (accounts) => ({
  type: RECEIVE_ACCOUNTS,
  accounts

});

export const postAccount = account => ({
  type: RECEIVE_ACCOUNTS,
  account
});

export const patchAccount = account => ({
  RECEIVE_ACCOUNT,
  account
});

export const removeAccount = (accountId) => ({
  type: REMOVE_ACCOUNT,
  accountId
});


export const requestAccounts = () => (
  AccountAPIUtil.fetchAccounts().then((accounts) => dispatchEvent(receiveAccounts(accounts)))
);

export const createAccount = account => (
  AccountAPIUtil.createAccount(account).then((account) => dispatch(postAccount(account)))
);

export const updateAccount = account => (
  AccountAPIUtil.updateAccount(account).then((account) => patchAccount(account))
);

export const deleteAccount = accountId => (
  AccountAPIUtil.deleteAccount(accountId).then((accountId) => removeAccount(accountId))
);