import React, {useEffect} from 'react'
import { Route } from 'react-router-dom'
import AccountIndexContainer from '../accounts/accounts_index_container'
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
        <AccountIndexContainer />
      </Route>
      <Route exact path="/overview/transactions">
        <TransactionIndexContainer />
      </Route>
      {/* <div className="accounts-filler"></div>
      <div className='chart-filler'></div> */}
    </main>
  )
} 
  

