import React, {useEffect} from 'react'
import { Route } from 'react-router-dom'
import AccountsIndex from '../accounts/accounts_index'
import TransactionIndexContainer from '../transactions/transaction_index_container'
import Modal from '../modal'
export default function overview({getAccounts, getTransactions}) {
  // const [accounts, setAccounts] = useState([])
  // setAccounts(getAccounts())
  useEffect(() => {
    getAccounts()
    getTransactions()
  }, [])

  return (
    <main className="overview-page">
      <Route exact path="/overview">
        <AccountsIndex />
      </Route>
      <Route exact path="/overview/transactions">
        <TransactionIndexContainer />
      </Route>
      {/* <div className="accounts-filler"></div>
      <div className='chart-filler'></div> */}
    </main>
  )
} 
  

