import React, { useEffect } from 'react'
import { useDispatch } from 'react-redux'
import { Route } from 'react-router-dom'
import AccountsIndex from '../accounts/accounts_index'
import TransactionIndex from '../transactions/transaction_index'
import {requestAccounts} from '../../actions/account_actions'
import { requestTransactions } from '../../actions/transaction_actions'

import Chart from '../transactions/chart'

export default function overview() {
  // dispatch and retrieve accounts
  const dispatch = useDispatch()
  const getAccounts = () => (dispatch(requestAccounts()))
  const getTransactions = () => (dispatch(requestTransactions()))

  // useEffect(() => {
  //   getAccounts()
  //   getTransactions()
  // }, []);
  getAccounts();
  getTransactions();

  return (
    <main className="overview-page">
      <Route exact path="/overview">
        <AccountsIndex />
      </Route>
      <Route exact path="/overview/spending">
        <Chart />
      </Route>
      <Route exact path="/overview/transactions">
        <TransactionIndex />
      </Route>




    </main>
  )
} 
  

