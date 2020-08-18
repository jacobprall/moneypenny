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
  type: RECEIVE_ACCOUNT,
  account
});

export const removeAccount = (accountId) => ({
  type: REMOVE_ACCOUNT,
  accountId
});


export const requestAccounts = () => dispatch => (
  AccountAPIUtil.fetchAccounts().then((accounts) => dispatch(receiveAccounts(accounts)))
);

export const createAccount = account => dispatch => (
  AccountAPIUtil.createAccount(account).then((account) => dispatch(postAccount(account)))
);

export const updateAccount = account => dispatch => (
  AccountAPIUtil.updateAccount(account).then((account) => dispatch(patchAccount(account)))
);

export const deleteAccount = accountId => dispatch => (
  AccountAPIUtil.deleteAccount(accountId).then((accountId) => dispatch(removeAccount(accountId)))
);