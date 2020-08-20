# == Schema Information
#
# Table name: transactions
#
#  id                   :bigint           not null, primary key
#  amount               :float            not null
#  date                 :datetime         not null
#  description          :string           not null
#  tags                 :string
#  transaction_category :string           not null
#  created_at           :datetime         not null
#  updated_at           :datetime         not null
#  account_id           :integer          not null
#
class Transaction < ApplicationRecord
  include PgSearch::Model
  pg_search_scope :search_for_transaction, against: [:description, :transaciton_category, :date]
  validates_presence_of :amount, :date, :description, :transaction_category, :account_id
  validates :transaction_category, inclusion: { in: %w(Housing Transportation Food Utilities Healthcare Personal Recreation/Entertainment Shopping Miscellaneous Other)}
  belongs_to :account

  def update_account(amt)
    self.account.balance += amt
  end
end
