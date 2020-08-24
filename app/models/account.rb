# == Schema Information
#
# Table name: accounts
#
#  id               :bigint           not null, primary key
#  account_category :string           not null
#  balance          :float            not null
#  debit            :boolean          not null
#  institution      :string           not null
#  label            :string           not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null
#  user_id          :integer          not null
#
# Indexes
#
#  index_accounts_on_user_id  (user_id)
#
class Account < ApplicationRecord
  CATEGORIES = [
    'Cash',
    'Credit Cards',
    'Loans',
    'Investments',
    'Property'
  ]
  INSTITUTIONS = [
    'Chase Bank',
    'J.P. Morgan',
    'Bank of America',
    'Merrill Lynch',
    'US Bank',
    'Citibank',
    'Wells Fargo',
    'Charles Schwab',
    'Fidelity', 
    'Discover', 
    'American Express',
    'Visa',
    'Other',
    'None'
  ]
  validates_presence_of :account_category, :institution, :label, :balance, :user_id 
  validates :account_category, inclusion: { in: CATEGORIES }
  validates :institution, inclusion: { in: INSTITUTIONS }
  validates :debit, inclusion: {in: [true, false]}

  belongs_to :user
  has_many :transactions
  has_many :goals

    def get_transaction_totals_by_category
    user = self.user
    allTransactions = user.transactions 
    allTransactions = allTransactions.map do |transaction|
      [transaction.transaction_category, transaction.amount]
    end

    categories = Hash.new(0)

    allTransactions.each do |transaction|
      categories[transaction[0]] += transaction[1]
    end

    categories
  end
  
end
